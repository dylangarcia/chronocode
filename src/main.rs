#![allow(dead_code)]

mod app;
mod cli;
mod gitignore;
mod recording;
mod renderer;
mod scanner;
mod state;
mod statistics;
mod watcher;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    // Handle --share mode
    if let Some(ref recording_file) = cli.share {
        handle_share(&cli, recording_file)?;
        return Ok(());
    }

    // Handle --viewer mode
    if cli.viewer {
        handle_viewer()?;
        return Ok(());
    }

    // Handle --replay mode
    if let Some(ref replay_file) = cli.replay {
        handle_replay(&cli, replay_file)?;
        return Ok(());
    }

    // Normal watch mode
    let mut app = app::App::new(&cli)?;
    app.run()?;

    Ok(())
}

fn handle_viewer() -> anyhow::Result<()> {
    // Look for replay.html next to the executable, then in common locations.
    let exe_dir = std::env::current_exe()?
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    let candidate_paths = [
        exe_dir.join("replay.html"),
        exe_dir.join("../share/chronocode/replay.html"),
        std::path::PathBuf::from("replay.html"),
    ];

    for candidate in &candidate_paths {
        if candidate.exists() {
            let dest = std::env::current_dir()?.join("chronocode-viewer.html");
            std::fs::copy(candidate, &dest)?;
            println!("Viewer copied to: {}", dest.display());
            open::that(&dest)?;
            return Ok(());
        }
    }

    println!("Opening web viewer...");
    println!("Note: replay.html was not found next to the executable.");
    println!("Place replay.html alongside the chronocode binary or in the current directory.");

    Ok(())
}

fn handle_share(cli: &cli::Cli, recording_file: &str) -> anyhow::Result<()> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    let path = std::path::Path::new(recording_file);
    if !path.exists() {
        anyhow::bail!("Recording file does not exist: {}", recording_file);
    }

    let raw = std::fs::read_to_string(path)?;

    // Parse and strip file contents to reduce size
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

    // Compress with deflate
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json.as_bytes())?;
    let compressed = encoder.finish()?;

    // Base64url encode
    let encoded = URL_SAFE_NO_PAD.encode(&compressed);

    // Build URL
    let base_url = if let Some(ref base) = cli.share_base_url {
        base.clone()
    } else {
        // Default to a local file URL
        let replay_html = std::env::current_dir()?.join("replay.html");
        if replay_html.exists() {
            format!("file://{}", replay_html.display())
        } else {
            "https://your-host.com/replay.html".to_string()
        }
    };

    let url = format!("{}#data={}", base_url, encoded);

    let raw_kb = raw.len() as f64 / 1024.0;
    let url_kb = url.len() as f64 / 1024.0;
    let ratio = (1.0 - url_kb / raw_kb) * 100.0;

    eprintln!("Recording: {} ({:.1} KB)", recording_file, raw_kb);
    eprintln!("URL size:  {:.1} KB ({:.0}% smaller)", url_kb, ratio);

    if url.len() > 100_000 {
        eprintln!(
            "Warning: URL is {:.0} KB -- may be too long for some browsers/services.",
            url_kb
        );
    }

    eprintln!();

    // Print the URL to stdout (so it can be piped)
    println!("{}", url);

    Ok(())
}

fn handle_replay(cli: &cli::Cli, replay_file: &str) -> anyhow::Result<()> {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use crossterm::event::{self, Event, KeyCode, KeyModifiers};
    use crossterm::execute;
    use crossterm::terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;

    let replay_path = Path::new(replay_file);
    if !replay_path.exists() {
        anyhow::bail!("Replay file does not exist: {}", replay_file);
    }

    let logger = recording::EventLogger::load_from_file(replay_path)?;
    let stats = logger.get_statistics();

    println!("Loading replay from: {}", replay_file);
    println!(
        "  {} events over {:.1}s",
        stats.total_events, stats.duration_seconds
    );
    println!(
        "  {} created, {} modified, {} deleted",
        stats.created, stats.modified, stats.deleted
    );
    println!("  Replay speed: {}x", cli.replay_speed);
    println!();

    // Build the initial state from the recording's initial_state field.
    let root_path = cli.path.canonicalize().unwrap_or_else(|_| cli.path.clone());
    let mut current_state = std::collections::HashMap::new();
    for item in &logger.initial_state {
        let path_str = item.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        let is_dir = item
            .get("is_dir")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let full_path = root_path.join(path_str);
        current_state.insert(
            full_path.clone(),
            state::FileInfo {
                path: full_path,
                size,
                modified: 0.0,
                is_dir,
                loc: 0,
            },
        );
    }

    // Set up terminal.
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let changes = state::ChangeSet::default();
    let previous_state = std::collections::HashMap::new();
    let replay_speed = cli.replay_speed;
    let events = &logger.events;
    let mut event_index = 0;
    let replay_start = Instant::now();

    loop {
        // Draw current state.
        terminal.draw(|frame| {
            renderer::render_ui(
                frame,
                &root_path,
                &current_state,
                &changes,
                &previous_state,
                None,
                false,
                cli.max_depth,
                cli.max_files,
                false,
            );
        })?;

        // Handle keyboard input.
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => {}
                }
            }
        }

        // Apply events whose timestamp has been reached.
        let elapsed = replay_start.elapsed().as_secs_f64() * replay_speed;
        while event_index < events.len() {
            let ev = &events[event_index];
            if ev.timestamp > elapsed {
                break;
            }
            let full_path = root_path.join(&ev.path);
            match ev.event_type {
                state::EventType::Created => {
                    current_state.insert(
                        full_path.clone(),
                        state::FileInfo {
                            path: full_path,
                            size: ev.size,
                            modified: 0.0,
                            is_dir: ev.is_dir,
                            loc: 0,
                        },
                    );
                }
                state::EventType::Modified => {
                    if let Some(info) = current_state.get_mut(&full_path) {
                        info.size = ev.size;
                    }
                }
                state::EventType::Deleted => {
                    current_state.remove(&full_path);
                }
            }
            event_index += 1;
        }

        // End replay once all events have been applied.
        if event_index >= events.len() {
            // Show final frame for a moment before exiting.
            std::thread::sleep(Duration::from_secs(2));
            break;
        }
    }

    // Cleanup.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    println!("Replay complete. {} events replayed.", events.len());

    Ok(())
}
