use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::cli::Cli;
use crate::git;
use crate::recording::EventLogger;
use crate::renderer;
use crate::scanner::ChangeTracker;
use crate::server;
use crate::statistics::StatisticsTracker;
use crate::watcher::{FileWatcher, WatchEvent};

/// Main application state and run loop.
pub struct App {
    pub root_path: PathBuf,
    pub tracker: ChangeTracker,
    pub show_stats: bool,
    pub is_recording: bool,
    pub auto_open_viewer: bool,
    pub max_depth: Option<usize>,
    pub max_files: Option<usize>,
    pub refresh_interval: Duration,
    pub scroll_offset: u16,
    pub total_tree_lines: u16,
    /// Whether the search input bar is actively accepting keystrokes.
    pub search_active: bool,
    /// The current search/filter query string.
    pub search_query: String,
    /// Last watcher error message, if any.
    pub last_error: Option<String>,
    /// Worktree paths discovered at startup (empty if disabled).
    pub worktree_paths: Vec<PathBuf>,
}

impl App {
    /// Build a new `App` from parsed CLI arguments.
    pub fn new(cli: &Cli) -> Result<Self> {
        let root_path = cli.path.canonicalize().unwrap_or_else(|_| cli.path.clone());

        // --- Event logger (recording — on by default) ---
        let event_logger = if !cli.no_record {
            let recordings_dir = root_path.join("recordings");
            std::fs::create_dir_all(&recordings_dir)?;

            let output_path = if let Some(ref name) = cli.record {
                let p = PathBuf::from(name);
                if p.parent().is_none_or(|par| par.as_os_str().is_empty()) {
                    recordings_dir.join(p)
                } else {
                    p
                }
            } else {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                recordings_dir.join(format!("recording_{}.json", ts))
            };

            Some(EventLogger::new(
                Some(output_path),
                Some(root_path.clone()),
                cli.content,
            ))
        } else {
            None
        };

        let is_recording = event_logger.is_some();

        // --- Statistics tracker ---
        let stats_tracker = if !cli.no_stats {
            Some(StatisticsTracker::new())
        } else {
            None
        };

        // --- Change tracker ---
        let mut tracker = ChangeTracker::new(
            root_path.clone(),
            !cli.no_gitignore,
            cli.all,
            event_logger,
            stats_tracker,
        );

        // --- Worktree discovery (on by default) ---
        let worktree_paths: Vec<PathBuf> = if !cli.no_worktrees {
            let worktrees = git::discover_worktrees(&root_path);
            if !worktrees.is_empty() {
                let paths: Vec<PathBuf> = worktrees.iter().map(|wt| wt.path.clone()).collect();
                eprintln!(
                    "Watching {} worktree{}:",
                    worktrees.len(),
                    if worktrees.len() == 1 { "" } else { "s" }
                );
                for wt in &worktrees {
                    eprintln!("  {} [{}]", wt.path.display(), wt.branch);
                }
                tracker.set_worktree_paths(paths.clone());
                paths
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let refresh_interval = Duration::from_secs_f64(cli.interval);

        Ok(Self {
            root_path,
            tracker,
            show_stats: !cli.no_stats,
            is_recording,
            auto_open_viewer: !cli.no_open && !cli.no_record,
            max_depth: cli.max_depth,
            max_files: cli.max_files,
            refresh_interval,
            scroll_offset: 0,
            total_tree_lines: 0,
            search_active: false,
            search_query: String::new(),
            last_error: None,
            worktree_paths,
        })
    }

    /// Compress a recording into a `#data=...` URL fragment.
    fn compress_recording(&self, recording_path: &std::path::Path) -> Result<String> {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let raw = std::fs::read_to_string(recording_path)?;

        // Parse and strip file contents to keep URL compact.
        let mut data: serde_json::Value = serde_json::from_str(&raw)?;
        if let Some(initial) = data.get_mut("initial_state").and_then(|v| v.as_array_mut()) {
            for item in initial.iter_mut() {
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("content");
                }
            }
        }
        if let Some(events) = data.get_mut("events").and_then(|v| v.as_array_mut()) {
            for event in events.iter_mut() {
                if let Some(obj) = event.as_object_mut() {
                    obj.remove("content");
                }
            }
        }

        let json = serde_json::to_string(&data)?;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(json.as_bytes())?;
        let compressed = encoder.finish()?;
        Ok(URL_SAFE_NO_PAD.encode(&compressed))
    }

    /// Open the recording in the web viewer via a local HTTP server.
    fn open_viewer(&self, recording_path: &std::path::Path) -> Result<()> {
        let encoded = self.compress_recording(recording_path)?;
        let fragment = format!("data={}", encoded);
        server::serve_and_open(Some(&fragment))
    }

    /// Run the main TUI event loop.
    pub fn run(&mut self) -> Result<()> {
        // 1. Initial scan — do NOT log these as events.
        //    Temporarily take the event_logger out so the first `update` call
        //    doesn't record the entire initial tree as "created" events.
        let logger_backup = self.tracker.event_logger.take();
        self.tracker.update(&self.root_path);

        // If we had a logger, capture the initial state and start recording,
        // then put it back.
        if let Some(mut logger) = logger_backup {
            logger.set_initial_state(&self.tracker.current_state);
            logger.start_recording();
            self.tracker.event_logger = Some(logger);
        }

        // 2. Set up the file watcher (root + any external worktrees).
        let mut watch_paths: Vec<&std::path::Path> = vec![&self.root_path];
        for wt in &self.worktree_paths {
            if !wt.starts_with(&self.root_path) {
                watch_paths.push(wt.as_path());
            }
        }
        let (_watcher, watch_rx) = FileWatcher::new_multi(&watch_paths, self.refresh_interval)?;

        // 3. Set up the terminal.
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend)?;

        // 4. Main loop.
        let mut last_update = Instant::now();
        let mut pending_change = false;

        loop {
            // --- Check for filesystem changes (non-blocking) ---
            // Drain all pending watcher events.
            loop {
                match watch_rx.try_recv() {
                    Ok(WatchEvent::FileChanged) => pending_change = true,
                    Ok(WatchEvent::Error(msg)) => {
                        self.last_error = Some(msg);
                    }
                    Err(_) => break,
                }
            }

            // Apply pending changes once the refresh interval has elapsed.
            if pending_change && last_update.elapsed() >= self.refresh_interval {
                self.tracker.update(&self.root_path);
                self.last_error = None;
                last_update = Instant::now();
                pending_change = false;
            }

            // --- Draw ---
            let scroll_offset = self.scroll_offset;
            let mut total_lines_out: u16 = 0;
            let search_query = self.search_query.clone();
            let search_active = self.search_active;
            let last_error = self.last_error.clone();
            terminal.draw(|frame| {
                let stats = self.tracker.stats_tracker.as_ref().map(|st| st.get_stats());
                total_lines_out = renderer::render_ui(
                    frame,
                    &self.root_path,
                    &self.tracker.current_state,
                    &self.tracker.changes,
                    &self.tracker.previous_state,
                    stats.as_ref(),
                    self.is_recording,
                    self.max_depth,
                    self.max_files,
                    self.show_stats,
                    scroll_offset,
                    &search_query,
                    search_active,
                    last_error.as_deref(),
                );
            })?;
            self.total_tree_lines = total_lines_out;

            // --- Handle keyboard events ---
            // Use a short poll timeout so we cycle back quickly to check
            // for filesystem changes.
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    // Ctrl+C always quits, regardless of search state.
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        break;
                    }

                    // Compute scroll dimensions for scroll key handling.
                    let term_height = terminal.size()?.height;
                    let stats_height: u16 =
                        if self.show_stats && self.tracker.stats_tracker.is_some() {
                            9
                        } else {
                            0
                        };
                    let overhead = 3 + 1 + stats_height + 1;
                    let viewport_height = term_height.saturating_sub(overhead);
                    let max_scroll = self.total_tree_lines.saturating_sub(viewport_height);
                    let half_page = viewport_height / 2;

                    if self.search_active {
                        // Search input mode: typing into the search bar.
                        match key.code {
                            KeyCode::Esc => {
                                self.search_active = false;
                                self.search_query.clear();
                            }
                            KeyCode::Enter => {
                                self.search_active = false;
                            }
                            KeyCode::Backspace => {
                                self.search_query.pop();
                            }
                            KeyCode::Char(c) => {
                                self.search_query.push(c);
                            }
                            _ => {}
                        }
                    } else if !self.search_query.is_empty() {
                        // Filter active but not in input mode — scroll + filter keys.
                        match key.code {
                            KeyCode::Esc => {
                                self.search_query.clear();
                            }
                            KeyCode::Char('/') => {
                                self.search_active = true;
                            }
                            KeyCode::Char('q') | KeyCode::Char('Q') => break,
                            KeyCode::Char('j') | KeyCode::Down => {
                                self.scroll_offset =
                                    self.scroll_offset.saturating_add(1).min(max_scroll);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                            }
                            KeyCode::PageDown => {
                                self.scroll_offset =
                                    self.scroll_offset.saturating_add(half_page).min(max_scroll);
                            }
                            KeyCode::PageUp => {
                                self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
                            }
                            KeyCode::Char('g') | KeyCode::Home => {
                                self.scroll_offset = 0;
                            }
                            KeyCode::Char('G') | KeyCode::End => {
                                self.scroll_offset = max_scroll;
                            }
                            _ => {}
                        }
                    } else {
                        // Normal mode — scroll + quit + search activation.
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => break,
                            KeyCode::Char('/') => {
                                self.search_active = true;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                self.scroll_offset =
                                    self.scroll_offset.saturating_add(1).min(max_scroll);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                            }
                            KeyCode::PageDown => {
                                self.scroll_offset =
                                    self.scroll_offset.saturating_add(half_page).min(max_scroll);
                            }
                            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                self.scroll_offset =
                                    self.scroll_offset.saturating_add(half_page).min(max_scroll);
                            }
                            KeyCode::PageUp => {
                                self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
                            }
                            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
                            }
                            KeyCode::Char('g') | KeyCode::Home => {
                                self.scroll_offset = 0;
                            }
                            KeyCode::Char('G') | KeyCode::End => {
                                self.scroll_offset = max_scroll;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // 5. Cleanup — restore the terminal.
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        // Print session summary.
        println!();

        if let Some(ref st) = self.tracker.stats_tracker {
            let stats = st.get_stats();
            println!("Session summary:");
            println!(
                "  Duration: {}",
                StatisticsTracker::format_duration(stats.session_duration)
            );
            println!(
                "  Events:  {} created, {} modified, {} deleted",
                stats.total_created, stats.total_modified, stats.total_deleted
            );
        }

        // Finalize recording and open viewer.
        // Extract data from the logger before calling self methods to avoid
        // borrow conflicts.
        let recording_info = if let Some(ref mut logger) = self.tracker.event_logger {
            logger.finalize();
            let event_count = logger.events.len();
            logger.output_path.clone().map(|p| (p, event_count))
        } else {
            None
        };

        if let Some((output_path, event_count)) = recording_info {
            println!(
                "Recording saved: {} ({} events)",
                output_path.display(),
                event_count
            );

            // Print a shareable command so users can send the recording.
            match self.compress_recording(&output_path) {
                Ok(encoded) => {
                    println!();
                    println!("Share this recording:");
                    println!("  chronocode --load {}", encoded);
                }
                Err(e) => {
                    eprintln!("Failed to generate share command: {}", e);
                }
            }

            if self.auto_open_viewer {
                if let Err(e) = self.open_viewer(&output_path) {
                    eprintln!("Failed to open viewer: {}", e);
                }
            }
        }

        println!();

        Ok(())
    }
}
