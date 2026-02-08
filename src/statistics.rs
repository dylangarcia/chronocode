use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Stats {
    pub session_duration: f64,
    pub total_created: u64,
    pub total_modified: u64,
    pub total_deleted: u64,
    pub current_files: i64,
    pub current_dirs: i64,
    pub peak_files: i64,
    pub peak_dirs: i64,
    pub events_per_minute: usize,
    /// Activity buckets: each element is (created_count, modified_count, deleted_count)
    /// for a time slice of the session.
    pub activity_buckets: Vec<(usize, usize, usize)>,
    /// Top file extensions by count, e.g. [(".rs", 10), (".toml", 3), ...].
    pub top_extensions: Vec<(String, usize)>,
}

pub struct StatisticsTracker {
    session_start: f64,
    total_created: u64,
    total_modified: u64,
    total_deleted: u64,
    current_files: i64,
    current_dirs: i64,
    total_bytes: i64,
    peak_files: i64,
    peak_dirs: i64,
    events_per_minute: Vec<(f64, String)>,
    /// All events ever recorded: (timestamp, event_type). Unlike events_per_minute
    /// this is never pruned, so we can build an activity timeline for the full session.
    all_events: Vec<(f64, String)>,
    /// Running count of file extensions seen across created events.
    extension_counts: HashMap<String, usize>,
}

impl StatisticsTracker {
    pub fn new() -> Self {
        Self {
            session_start: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            total_created: 0,
            total_modified: 0,
            total_deleted: 0,
            current_files: 0,
            current_dirs: 0,
            total_bytes: 0,
            peak_files: 0,
            peak_dirs: 0,
            events_per_minute: Vec::new(),
            all_events: Vec::new(),
            extension_counts: HashMap::new(),
        }
    }

    pub fn record_event(
        &mut self,
        event_type: &str,
        size: u64,
        is_dir: bool,
        extension: Option<&str>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        self.events_per_minute.push((now, event_type.to_string()));
        self.all_events.push((now, event_type.to_string()));

        let cutoff = now - 60.0;
        self.events_per_minute.retain(|(ts, _)| *ts >= cutoff);

        // Track file extension counts for non-directory events.
        if !is_dir {
            if let Some(ext) = extension {
                let ext_key = if ext.starts_with('.') {
                    ext.to_string()
                } else {
                    format!(".{}", ext)
                };
                match event_type {
                    "created" => {
                        *self.extension_counts.entry(ext_key).or_insert(0) += 1;
                    }
                    "deleted" => {
                        let count = self.extension_counts.entry(ext_key).or_insert(0);
                        *count = count.saturating_sub(1);
                    }
                    _ => {}
                }
            }
        }

        match event_type {
            "created" => {
                self.total_created += 1;
                self.total_bytes += size as i64;
                if is_dir {
                    self.current_dirs += 1;
                    if self.current_dirs > self.peak_dirs {
                        self.peak_dirs = self.current_dirs;
                    }
                } else {
                    self.current_files += 1;
                    if self.current_files > self.peak_files {
                        self.peak_files = self.current_files;
                    }
                }
            }
            "deleted" => {
                self.total_deleted += 1;
                self.total_bytes -= size as i64;
                if is_dir {
                    self.current_dirs -= 1;
                } else {
                    self.current_files -= 1;
                }
            }
            "modified" => {
                self.total_modified += 1;
            }
            _ => {}
        }
    }

    /// Divide the session duration into `num_buckets` equal time slices and
    /// return `(created, modified, deleted)` counts for each bucket.
    pub fn get_activity_buckets(&self, num_buckets: usize) -> Vec<(usize, usize, usize)> {
        if num_buckets == 0 {
            return Vec::new();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let duration = now - self.session_start;

        if duration <= 0.0 || self.all_events.is_empty() {
            return vec![(0, 0, 0); num_buckets];
        }

        let bucket_width = duration / num_buckets as f64;
        let mut buckets = vec![(0usize, 0usize, 0usize); num_buckets];

        for (ts, event_type) in &self.all_events {
            let elapsed = ts - self.session_start;
            let idx = ((elapsed / bucket_width) as usize).min(num_buckets - 1);

            match event_type.as_str() {
                "created" => buckets[idx].0 += 1,
                "modified" => buckets[idx].1 += 1,
                "deleted" => buckets[idx].2 += 1,
                _ => {}
            }
        }

        buckets
    }

    /// Return the top `n` file extensions by count.
    pub fn get_top_extensions(&self, n: usize) -> Vec<(String, usize)> {
        let mut exts: Vec<(String, usize)> = self
            .extension_counts
            .iter()
            .filter(|(_, &count)| count > 0)
            .map(|(ext, &count)| (ext.clone(), count))
            .collect();

        exts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        exts.truncate(n);
        exts
    }

    pub fn get_stats(&self) -> Stats {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        Stats {
            session_duration: now - self.session_start,
            total_created: self.total_created,
            total_modified: self.total_modified,
            total_deleted: self.total_deleted,
            current_files: self.current_files,
            current_dirs: self.current_dirs,
            peak_files: self.peak_files,
            peak_dirs: self.peak_dirs,
            events_per_minute: self.events_per_minute.len(),
            activity_buckets: self.get_activity_buckets(50),
            top_extensions: self.get_top_extensions(5),
        }
    }

    pub fn format_duration(seconds: f64) -> String {
        let total_secs = seconds as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let secs = total_secs % 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, secs)
        } else {
            format!("{}s", secs)
        }
    }
}
