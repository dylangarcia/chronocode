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
    use std::process::{Command, Stdio};
    use std::time::Duration;

    const REPLAY_HTML: &str = include_str!("../replay.html");

    let tmp_dir = std::env::temp_dir().join("chronocode-viewer");
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("index.html"), REPLAY_HTML)?;

    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };

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

    std::thread::sleep(Duration::from_millis(300));

    let url = format!("http://127.0.0.1:{}/", port);
    println!("Viewer running at {}", url);
    open::that(&url)?;

    // Give the browser time to load the page and all assets,
    // then tear down the server. Once loaded, the page is self-contained.
    std::thread::sleep(Duration::from_secs(3));
    let _ = server.kill();
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(())
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
    use std::process::{Command, Stdio};
    use std::time::Duration;

    const REPLAY_HTML: &str = include_str!("../replay.html");

    // Write the embedded HTML to a temp directory.
    let tmp_dir = std::env::temp_dir().join("chronocode-viewer");
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("index.html"), REPLAY_HTML)?;

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

    let url = format!("http://127.0.0.1:{}/#data={}", port, data);

    println!("Opening viewer at http://127.0.0.1:{} ...", port);
    open::that(&url)?;

    // Give the browser time to load the page and all assets,
    // then tear down the server. Once loaded, the page is self-contained.
    std::thread::sleep(Duration::from_secs(3));
    let _ = server.kill();
    let _ = std::fs::remove_dir_all(&tmp_dir);

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
