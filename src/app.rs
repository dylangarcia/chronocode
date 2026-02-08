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
use crate::recording::EventLogger;
use crate::renderer;
use crate::scanner::ChangeTracker;
use crate::statistics::StatisticsTracker;
use crate::watcher::{FileWatcher, WatchEvent};

/// The replay viewer HTML, embedded into the binary at compile time.
const REPLAY_HTML: &str = include_str!("../replay.html");

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
    pub running: bool,
    pub scroll_offset: u16,
    pub total_tree_lines: u16,
    /// Whether the search input bar is actively accepting keystrokes.
    pub search_active: bool,
    /// The current search/filter query string.
    pub search_query: String,
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
        let tracker = ChangeTracker::new(
            root_path.clone(),
            !cli.no_gitignore,
            cli.all,
            event_logger,
            stats_tracker,
        );

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
            running: true,
            scroll_offset: 0,
            total_tree_lines: 0,
            search_active: false,
            search_query: String::new(),
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
    /// The HTML is embedded in the binary — no external files needed.
    fn open_viewer(&self, recording_path: &std::path::Path) -> Result<()> {
        use std::process::{Command, Stdio};

        let encoded = self.compress_recording(recording_path)?;

        // Write the embedded HTML to a temp directory.
        let tmp_dir = std::env::temp_dir().join("chronocode-viewer");
        std::fs::create_dir_all(&tmp_dir)?;
        let html_path = tmp_dir.join("index.html");
        std::fs::write(&html_path, REPLAY_HTML)?;

        // Pick a free port.
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            listener.local_addr()?.port()
        };

        // Spawn a local server. Try python3 first, then npx serve.
        let mut server = Command::new("python3")
            .args([
                "-m",
                "http.server",
                &port.to_string(),
                "--bind",
                "127.0.0.1",
            ])
            .current_dir(&tmp_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("npx")
                    .args(["serve", "-l", &port.to_string(), "-s", "."])
                    .current_dir(&tmp_dir)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            })
            .map_err(|_| anyhow::anyhow!("Could not start a local server (need python3 or npx)"))?;

        // Give the server a moment to bind.
        std::thread::sleep(Duration::from_millis(300));

        let url = format!("http://127.0.0.1:{}/#data={}", port, encoded);

        println!("Opening viewer at http://127.0.0.1:{} ...", port);
        open::that(&url)?;

        // Give the browser time to load the page and all assets,
        // then tear down the server. Once loaded, the page is self-contained.
        std::thread::sleep(Duration::from_secs(3));
        let _ = server.kill();
        let _ = std::fs::remove_dir_all(&tmp_dir);

        Ok(())
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

        // 2. Set up the file watcher.
        let (_watcher, watch_rx) = FileWatcher::new(&self.root_path, self.refresh_interval)?;

        // 3. Set up the terminal.
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend)?;

        // 4. Main loop.
        let mut last_update = Instant::now();

        loop {
            // --- Draw ---
            let scroll_offset = self.scroll_offset;
            let mut total_lines_out: u16 = 0;
            let search_query = self.search_query.clone();
            let search_active = self.search_active;
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
                );
            })?;
            self.total_tree_lines = total_lines_out;

            // --- Handle keyboard events ---
            if event::poll(Duration::from_millis(100))? {
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

            // --- Check for filesystem changes (non-blocking) ---
            if let Ok(WatchEvent::FileChanged) = watch_rx.try_recv() {
                if last_update.elapsed() >= self.refresh_interval {
                    // Drain any additional queued events.
                    while watch_rx.try_recv().is_ok() {}
                    self.tracker.update(&self.root_path);
                    last_update = Instant::now();
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
        if let Some(ref logger) = self.tracker.event_logger {
            logger.finalize();
            let event_count = logger.events.len();

            if let Some(ref output_path) = logger.output_path {
                println!(
                    "Recording saved: {} ({} events)",
                    output_path.display(),
                    event_count
                );

                // Print a shareable command so users can send the recording.
                match self.compress_recording(output_path) {
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
                    if let Err(e) = self.open_viewer(output_path) {
                        eprintln!("Failed to open viewer: {}", e);
                    }
                }
            }
        }

        println!();

        Ok(())
    }
}
