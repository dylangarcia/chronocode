use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Result;

/// The replay viewer HTML, embedded into the binary at compile time.
const REPLAY_HTML: &str = include_str!("../replay.html");

/// Spin up a local HTTP server, open the viewer in the browser, wait for it to
/// load, then tear down the server.
///
/// If `url_fragment` is `Some(frag)`, the opened URL will be
/// `http://127.0.0.1:{port}/#{frag}`.  Otherwise, the root URL is opened.
pub fn serve_and_open(url_fragment: Option<&str>) -> Result<()> {
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

    let url = match url_fragment {
        Some(frag) => format!("http://127.0.0.1:{}/#{}", port, frag),
        None => format!("http://127.0.0.1:{}/", port),
    };

    println!("Opening viewer at http://127.0.0.1:{} ...", port);
    open::that(&url)?;

    // Give the browser time to load the page and all assets,
    // then tear down the server. Once loaded, the page is self-contained.
    std::thread::sleep(Duration::from_secs(3));
    let _ = server.kill();
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(())
}
