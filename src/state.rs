use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Maximum file size (in bytes) for reading content into memory.
pub const MAX_CONTENT_SIZE: u64 = 100 * 1024;

// ---------------------------------------------------------------------------
// EventType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Created,
    Modified,
    Deleted,
}

// ---------------------------------------------------------------------------
// FileInfo
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub modified: f64,
    pub is_dir: bool,
    pub loc: usize,
}

impl Default for FileInfo {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            size: 0,
            modified: 0.0,
            is_dir: false,
            loc: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// FileEvent
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileEvent {
    pub timestamp: f64,
    #[serde(rename = "event_type")]
    pub event_type: EventType,
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub loc: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ---------------------------------------------------------------------------
// ChangeSet
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ChangeSet {
    pub added: HashSet<PathBuf>,
    pub modified: HashSet<PathBuf>,
    pub deleted: HashSet<PathBuf>,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Return an emoji representing the file type based on its extension or name.
pub fn get_file_emoji(filename: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "\u{1F4C1}"; // 📁
    }

    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "py" => "\u{1F40D}",                                             // 🐍
        "js" | "jsx" => "\u{1F4DC}",                                     // 📜
        "ts" | "tsx" => "\u{1F4D8}",                                     // 📘
        "json" => "\u{1F4CB}",                                           // 📋
        "md" => "\u{1F4DD}",                                             // 📝
        "txt" => "\u{1F4C4}",                                            // 📄
        "png" | "jpg" | "jpeg" | "gif" | "svg" => "\u{1F5BC}\u{FE0F} ",  // 🖼️
        "mp4" => "\u{1F3AC}",                                            // 🎬
        "mp3" => "\u{1F3B5}",                                            // 🎵
        "zip" | "tar" | "gz" => "\u{1F4E6}",                             // 📦
        "yaml" | "yml" | "toml" | "ini" | "conf" => "\u{2699}\u{FE0F} ", // ⚙️
        "gitignore" => "\u{1F500}",                                      // 🔀
        "lock" => "\u{1F512}",                                           // 🔒
        "pdf" | "doc" | "docx" => "\u{1F4DA}",                           // 📚
        _ => "\u{1F4C4}",                                                // 📄
    }
}

/// Count the number of lines in a file. Returns 0 on any error.
pub fn get_loc(filepath: &Path) -> usize {
    let file = match fs::File::open(filepath) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let reader = BufReader::new(file);
    reader.lines().count()
}

/// Format a byte count as a human-readable string (e.g. "1.5 KB", "12 MB").
pub fn format_size(size_bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    let mut size = size_bytes as f64;
    for &unit in UNITS {
        if size < 1024.0 {
            return if size < 10.0 && unit != "B" {
                format!("{:.1} {}", size, unit)
            } else {
                format!("{:.0} {}", size, unit)
            };
        }
        size /= 1024.0;
    }

    // Larger than TB – just show TB
    if size < 10.0 {
        format!("{:.1} TB", size)
    } else {
        format!("{:.0} TB", size)
    }
}

/// Return a color name appropriate for the given file size.
pub fn get_size_color(size_bytes: u64) -> &'static str {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;

    if size_bytes < KB {
        "dim"
    } else if size_bytes < MB {
        "cyan"
    } else if size_bytes < 100 * MB {
        "yellow"
    } else {
        "red"
    }
}

/// Format a signed delta value for display, returning (formatted_string, color).
///
/// For size deltas `is_size` should be `true` (uses `format_size`); for LOC
/// deltas it should be `false` (plain number formatting).
pub fn format_delta(value: i64, is_size: bool) -> (String, &'static str) {
    if value == 0 {
        return (String::new(), "dim");
    }

    let color = if value > 0 { "green" } else { "red" };
    let sign = if value > 0 { "+" } else { "-" };
    let abs = value.unsigned_abs();

    let formatted = if is_size {
        format!("{}{}", sign, format_size(abs))
    } else {
        format!("{}{}", sign, abs)
    };

    (formatted, color)
}

/// Set of extensions considered to be text/source files.
const TEXT_EXTENSIONS: &[&str] = &[
    "py",
    "js",
    "ts",
    "jsx",
    "tsx",
    "json",
    "md",
    "txt",
    "html",
    "css",
    "yaml",
    "yml",
    "toml",
    "ini",
    "conf",
    "cfg",
    "sh",
    "bash",
    "zsh",
    "xml",
    "svg",
    "sql",
    "rb",
    "go",
    "rs",
    "java",
    "c",
    "cpp",
    "h",
    "hpp",
    "swift",
    "kt",
    "scala",
    "php",
    "pl",
    "pm",
    "r",
    "lua",
    "vim",
    "el",
    "clj",
    "cljs",
    "ex",
    "exs",
    "erl",
    "hrl",
    "hs",
    "ml",
    "mli",
    "fs",
    "fsi",
    "vue",
    "svelte",
    "astro",
    "graphql",
    "gql",
    "proto",
    "dockerfile",
    "makefile",
    "cmake",
    "gradle",
    "pom",
    "env",
    "gitignore",
    "gitattributes",
];

/// Exact filenames (case-sensitive) that are considered text files regardless
/// of extension.
const TEXT_FILENAMES: &[&str] = &["Makefile", "Dockerfile", "Gemfile", "Rakefile", "Procfile"];

/// Determine whether a path points to a text/source file based on its
/// extension or exact filename.
pub fn is_text_file(path: &Path) -> bool {
    // Check exact filename match first.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if TEXT_FILENAMES.contains(&name) {
            return true;
        }
    }

    // Then check extension.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        TEXT_EXTENSIONS.contains(&ext_lower.as_str())
    } else {
        false
    }
}

/// Read the contents of a text file if it is a recognised text file and does
/// not exceed `max_size` bytes. Returns `None` on any error or if the file
/// is not a text file.
pub fn read_file_content(path: &Path, max_size: u64) -> Option<String> {
    if !is_text_file(path) {
        return None;
    }

    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > max_size {
        return None;
    }

    fs::read_to_string(path).ok()
}

/// Format a lines-of-code count for compact display.
///
/// - 0       -> ""
/// - < 1000  -> "42L"
/// - < 1M    -> "12kL"
/// - >= 1M   -> "3ML"
pub fn format_loc(loc: usize) -> String {
    if loc == 0 {
        String::new()
    } else if loc < 1_000 {
        format!("{}L", loc)
    } else if loc < 1_000_000 {
        format!("{}kL", loc / 1_000)
    } else {
        format!("{}ML", loc / 1_000_000)
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers for EventType
// ---------------------------------------------------------------------------

impl EventType {
    /// Return the string representation matching the serde serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Created => "created",
            EventType::Modified => "modified",
            EventType::Deleted => "deleted",
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers for FileEvent (JSON interop)
// ---------------------------------------------------------------------------

impl FileEvent {
    /// Serialize this event to a `serde_json::Value`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("FileEvent should always be serializable")
    }

    /// Deserialize a `FileEvent` from a `serde_json::Value`.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_serialization() {
        assert_eq!(
            serde_json::to_string(&EventType::Created).unwrap(),
            "\"created\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::Modified).unwrap(),
            "\"modified\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::Deleted).unwrap(),
            "\"deleted\""
        );
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(10240), "10 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_get_size_color() {
        assert_eq!(get_size_color(500), "dim");
        assert_eq!(get_size_color(2048), "cyan");
        assert_eq!(get_size_color(5 * 1024 * 1024), "yellow");
        assert_eq!(get_size_color(200 * 1024 * 1024), "red");
    }

    #[test]
    fn test_format_delta() {
        assert_eq!(format_delta(0, false), (String::new(), "dim"));
        assert_eq!(format_delta(42, false), ("+42".to_string(), "green"));
        assert_eq!(format_delta(-7, false), ("-7".to_string(), "red"));
        assert_eq!(format_delta(2048, true), ("+2.0 KB".to_string(), "green"));
        assert_eq!(format_delta(-1024, true), ("-1.0 KB".to_string(), "red"));
    }

    #[test]
    fn test_format_loc() {
        assert_eq!(format_loc(0), "");
        assert_eq!(format_loc(42), "42L");
        assert_eq!(format_loc(1500), "1kL");
        assert_eq!(format_loc(2_500_000), "2ML");
    }

    #[test]
    fn test_get_file_emoji() {
        assert_eq!(get_file_emoji("dir", true), "\u{1F4C1}");
        assert_eq!(get_file_emoji("main.py", false), "\u{1F40D}");
        assert_eq!(get_file_emoji("app.tsx", false), "\u{1F4D8}");
        assert_eq!(get_file_emoji("unknown.xyz", false), "\u{1F4C4}");
    }

    #[test]
    fn test_is_text_file() {
        assert!(is_text_file(Path::new("main.py")));
        assert!(is_text_file(Path::new("Makefile")));
        assert!(is_text_file(Path::new("component.vue")));
        assert!(!is_text_file(Path::new("image.png")));
        assert!(!is_text_file(Path::new("binary.exe")));
    }

    #[test]
    fn test_changeset_default() {
        let cs = ChangeSet::default();
        assert!(cs.added.is_empty());
        assert!(cs.modified.is_empty());
        assert!(cs.deleted.is_empty());
    }
}
