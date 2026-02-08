use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

pub enum WatchEvent {
    FileChanged,
    #[allow(dead_code)] // Will be handled in Task 3
    Error(String),
}

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Create a new file watcher that monitors multiple paths recursively.
    /// All paths share a single watcher and channel.
    pub fn new_multi(
        paths: &[&Path],
        _debounce_duration: Duration,
    ) -> anyhow::Result<(Self, mpsc::Receiver<WatchEvent>)> {
        let (tx, rx) = mpsc::channel();

        let sender = tx.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                match res {
                    Ok(_event) => {
                        // Send a generic "something changed" signal
                        // The actual change detection is done by rescanning (like the Python version)
                        let _ = sender.send(WatchEvent::FileChanged);
                    }
                    Err(e) => {
                        let _ = sender.send(WatchEvent::Error(e.to_string()));
                    }
                }
            })?;

        for path in paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        Ok((Self { _watcher: watcher }, rx))
    }
}
