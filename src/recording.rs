use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde_json::{self, json, Value};

use crate::state::{read_file_content, EventType, FileEvent, FileInfo, MAX_CONTENT_SIZE};

/// Statistics about a recording session.
pub struct RecordingStats {
    pub total_events: usize,
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
    pub duration_seconds: f64,
}

/// Logs file system events for later replay with continuous file writing.
pub struct EventLogger {
    pub events: Vec<FileEvent>,
    pub initial_state: Vec<Value>,
    pub start_time: Option<f64>,
    pub output_path: Option<PathBuf>,
    pub root_path: Option<PathBuf>,
    pub record_content: bool,
}

impl EventLogger {
    /// Create a new EventLogger.
    pub fn new(
        output_path: Option<PathBuf>,
        root_path: Option<PathBuf>,
        record_content: bool,
    ) -> Self {
        Self {
            events: Vec::new(),
            initial_state: Vec::new(),
            start_time: None,
            output_path,
            root_path,
            record_content,
        }
    }

    /// Convert an absolute path to a relative path from root.
    fn to_relative_path(&self, path: &Path) -> String {
        let Some(root) = &self.root_path else {
            return path.to_string_lossy().into_owned();
        };

        match path.strip_prefix(root) {
            Ok(rel) => {
                let s = rel.to_string_lossy();
                if s.is_empty() {
                    ".".to_string()
                } else {
                    s.into_owned()
                }
            }
            Err(_) => path.to_string_lossy().into_owned(),
        }
    }

    /// Capture the initial state of all files.
    pub fn set_initial_state(&mut self, file_infos: &HashMap<PathBuf, FileInfo>) {
        self.initial_state.clear();

        for (path, info) in file_infos {
            let mut item = json!({
                "path": self.to_relative_path(path),
                "size": info.size,
                "is_dir": info.is_dir,
            });

            // Capture content for text files when enabled
            if self.record_content && !info.is_dir {
                if let Some(content) = read_file_content(path, MAX_CONTENT_SIZE) {
                    item["content"] = Value::String(content);
                }
            }

            self.initial_state.push(item);
        }
    }

    /// Start recording events and initialize the output file.
    pub fn start_recording(&mut self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_secs_f64();
        self.start_time = Some(now);
        self.events.clear();

        // Write initial state to file
        if self.output_path.is_some() {
            self.write_file();
        }
    }

    /// Write the complete valid JSON file atomically (write to .tmp, then rename).
    fn write_file(&self) {
        let Some(output_path) = &self.output_path else {
            return;
        };

        let data = json!({
            "start_time": self.start_time,
            "initial_state": self.initial_state,
            "events": self.events.iter().map(|e| e.to_json()).collect::<Vec<Value>>(),
        });

        let temp_path = output_path.with_extension("tmp");
        let json_bytes = serde_json::to_string(&data).expect("failed to serialize JSON");

        if let Err(e) = fs::write(&temp_path, json_bytes) {
            eprintln!("Warning: failed to write temp file: {e}");
            return;
        }

        if let Err(e) = fs::rename(&temp_path, output_path) {
            eprintln!("Warning: failed to rename temp file: {e}");
        }
    }

    /// Log a file system event and write to file immediately.
    pub fn log_event(&mut self, event_type: EventType, path: &Path, size: u64, is_dir: bool) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_secs_f64();

        let start = self.start_time.unwrap_or(now);
        if self.start_time.is_none() {
            self.start_time = Some(now);
        }

        let timestamp = now - start;

        // Capture content for created/modified text files when enabled
        let content = if self.record_content
            && matches!(event_type, EventType::Created | EventType::Modified)
            && !is_dir
        {
            read_file_content(path, MAX_CONTENT_SIZE)
        } else {
            None
        };

        let event = FileEvent {
            timestamp,
            event_type,
            path: self.to_relative_path(path),
            size,
            is_dir,
            content,
        };
        self.events.push(event);

        // Write complete valid JSON file after each event
        if self.output_path.is_some() {
            self.write_file();
        }
    }

    /// Finalize the recording (writes final state).
    pub fn finalize(&self) {
        if self.output_path.is_some() {
            self.write_file();
        }
    }

    /// Load a recording from a JSON file.
    pub fn load_from_file(filepath: &Path) -> Result<Self> {
        let contents = fs::read_to_string(filepath)
            .with_context(|| format!("reading {}", filepath.display()))?;
        let data: Value =
            serde_json::from_str(&contents).with_context(|| "parsing recording JSON")?;

        let start_time = data.get("start_time").and_then(|v| v.as_f64());

        let initial_state: Vec<Value> = data
            .get("initial_state")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let events: Vec<FileEvent> = data
            .get("events")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(FileEvent::from_json).collect())
            .unwrap_or_default();

        Ok(Self {
            events,
            initial_state,
            start_time,
            output_path: None,
            root_path: None,
            record_content: false,
        })
    }

    /// Get statistics about recorded events.
    pub fn get_statistics(&self) -> RecordingStats {
        let total_events = self.events.len();
        let created = self
            .events
            .iter()
            .filter(|e| e.event_type == EventType::Created)
            .count();
        let modified = self
            .events
            .iter()
            .filter(|e| e.event_type == EventType::Modified)
            .count();
        let deleted = self
            .events
            .iter()
            .filter(|e| e.event_type == EventType::Deleted)
            .count();
        let duration_seconds = self.events.last().map(|e| e.timestamp).unwrap_or(0.0);

        RecordingStats {
            total_events,
            created,
            modified,
            deleted,
            duration_seconds,
        }
    }
}
