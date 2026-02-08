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
    pub total_bytes: i64,
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
        }
    }

    pub fn record_event(&mut self, event_type: &str, size: u64, is_dir: bool) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        self.events_per_minute.push((now, event_type.to_string()));

        let cutoff = now - 60.0;
        self.events_per_minute.retain(|(ts, _)| *ts >= cutoff);

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
            total_bytes: self.total_bytes,
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
