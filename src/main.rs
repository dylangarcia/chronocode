mod app;
mod cli;
mod git;
mod gitignore;
mod recording;
mod renderer;
mod scanner;
mod server;
mod state;
mod statistics;
mod watcher;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    // Handle --share mode
    if let Some(ref recording_file) = cli.share {
        handle_share(recording_file)?;
        return Ok(());
    }

    // Handle --load mode
    if let Some(ref data) = cli.load {
        handle_load(data)?;
        return Ok(());
    }

    // Handle --viewer mode
    if cli.viewer {
        handle_viewer()?;
        return Ok(());
    }

    // Handle --git mode
    if let Some(ref git_spec) = cli.git {
        handle_git(git_spec, &cli)?;
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
    server::serve_and_open(None)
}

fn handle_share(recording_file: &str) -> anyhow::Result<()> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let path = std::path::Path::new(recording_file);
    if !path.exists() {
        anyhow::bail!("Recording file does not exist: {}", recording_file);
    }

    let raw = std::fs::read_to_string(path)?;

    // Parse and strip file contents to reduce size.
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

    // Compress with zlib (matches pako.deflate/inflate in the browser).
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json.as_bytes())?;
    let compressed = encoder.finish()?;

    // Base64url encode.
    let encoded = URL_SAFE_NO_PAD.encode(&compressed);

    let raw_kb = raw.len() as f64 / 1024.0;
    let encoded_kb = encoded.len() as f64 / 1024.0;
    let ratio = (1.0 - encoded_kb / raw_kb) * 100.0;

    eprintln!("Recording: {} ({:.1} KB)", recording_file, raw_kb);
    eprintln!("Compressed: {:.1} KB ({:.0}% smaller)", encoded_kb, ratio);
    eprintln!();

    // Print a command the recipient can run to view the recording.
    println!("chronocode --load {}", encoded);

    Ok(())
}

fn handle_load(data: &str) -> anyhow::Result<()> {
    let fragment = format!("data={}", data);
    server::serve_and_open(Some(&fragment))
}

fn handle_git(spec: &str, cli: &cli::Cli) -> anyhow::Result<()> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let repo_path = cli.path.canonicalize().unwrap_or_else(|_| cli.path.clone());
    let recording = git::generate_recording(spec, &repo_path)?;

    let stats = {
        let created = recording
            .events
            .iter()
            .filter(|e| e.event_type == state::EventType::Created)
            .count();
        let modified = recording
            .events
            .iter()
            .filter(|e| e.event_type == state::EventType::Modified)
            .count();
        let deleted = recording
            .events
            .iter()
            .filter(|e| e.event_type == state::EventType::Deleted)
            .count();
        (recording.events.len(), created, modified, deleted)
    };

    eprintln!(
        "  {} commits, {} events ({} created, {} modified, {} deleted)",
        recording.commit_count, stats.0, stats.1, stats.2, stats.3
    );

    // Build the recording JSON.
    let data = serde_json::json!({
        "start_time": recording.start_time,
        "initial_state": recording.initial_state,
        "events": recording.events.iter().map(|e| e.to_json()).collect::<Vec<serde_json::Value>>(),
    });

    // Save to file.
    let recordings_dir = repo_path.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let output_path = recordings_dir.join(format!("git_recording_{}.json", ts));
    let json_str = serde_json::to_string(&data)?;
    std::fs::write(&output_path, &json_str)?;

    eprintln!(
        "Recording saved: {} ({} events)",
        output_path.display(),
        stats.0
    );

    // Compress and print share command.
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json_str.as_bytes())?;
    let compressed = encoder.finish()?;
    let encoded = URL_SAFE_NO_PAD.encode(&compressed);

    eprintln!();
    eprintln!("Share this recording:");
    eprintln!("  chronocode --load {}", encoded);

    // Open the viewer.
    if !cli.no_open {
        let fragment = format!("data={}", encoded);
        server::serve_and_open(Some(&fragment))?;
    }

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
        let loc = item.get("loc").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let full_path = root_path.join(path_str);
        current_state.insert(
            full_path,
            state::FileInfo {
                size,
                modified: 0.0,
                is_dir,
                loc,
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
                0,
                "",
                false,
                None,
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
                        full_path,
                        state::FileInfo {
                            size: ev.size,
                            modified: 0.0,
                            is_dir: ev.is_dir,
                            loc: ev.loc,
                        },
                    );
                }
                state::EventType::Modified => {
                    if let Some(info) = current_state.get_mut(&full_path) {
                        info.size = ev.size;
                        info.loc = ev.loc;
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
