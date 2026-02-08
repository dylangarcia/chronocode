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

/// Main application state and run loop.
pub struct App {
    pub root_path: PathBuf,
    pub tracker: ChangeTracker,
    pub show_stats: bool,
    pub is_recording: bool,
    pub max_depth: Option<usize>,
    pub max_files: Option<usize>,
    pub refresh_interval: Duration,
    pub running: bool,
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
            max_depth: cli.max_depth,
            max_files: cli.max_files,
            refresh_interval,
            running: true,
        })
    }

    /// Generate a shareable URL from a recording file.
    fn generate_share_url(&self, recording_path: &std::path::Path) -> Result<String> {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        use flate2::write::DeflateEncoder;
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

        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(json.as_bytes())?;
        let compressed = encoder.finish()?;
        let encoded = URL_SAFE_NO_PAD.encode(&compressed);

        // Find replay.html relative to the recording or CWD.
        let replay_html = self.root_path.join("replay.html");
        let base_url = if replay_html.exists() {
            format!("file://{}", replay_html.display())
        } else {
            let cwd_replay = std::env::current_dir()
                .unwrap_or_default()
                .join("replay.html");
            if cwd_replay.exists() {
                format!("file://{}", cwd_replay.display())
            } else {
                "file:///path/to/replay.html".to_string()
            }
        };

        Ok(format!("{}#data={}", base_url, encoded))
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
            terminal.draw(|frame| {
                let stats = self.tracker.stats_tracker.as_ref().map(|st| st.get_stats());
                renderer::render_ui(
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
                );
            })?;

            // --- Handle keyboard events ---
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }
                        _ => {}
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

        // Finalize recording and print location + shareable link.
        if let Some(ref logger) = self.tracker.event_logger {
            logger.finalize();
            let event_count = logger.events.len();

            if let Some(ref output_path) = logger.output_path {
                println!();
                println!(
                    "Recording saved: {} ({} events)",
                    output_path.display(),
                    event_count
                );

                // Generate a shareable viewer URL.
                if let Ok(url) = self.generate_share_url(output_path) {
                    // Print an OSC 8 clickable hyperlink for terminals that support it.
                    println!();
                    println!(
                        "  Open recording: \x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
                        url, url
                    );
                }
            }
        }

        println!();

        Ok(())
    }
}
