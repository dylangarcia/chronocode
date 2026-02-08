use clap::Parser;
use std::path::PathBuf;

/// Watch directory structure and file changes in real-time
#[derive(Parser, Debug)]
#[command(name = "chronocode")]
#[command(version)]
#[command(about = "Watch directory structure and file changes in real-time")]
pub struct Cli {
    /// Directory to watch
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Show hidden files and directories
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Refresh interval in seconds
    #[arg(short = 'i', long = "interval", default_value_t = 0.25)]
    pub interval: f64,

    /// Maximum files per directory
    #[arg(short = 'f', long = "max-files")]
    pub max_files: Option<usize>,

    /// Maximum tree depth
    #[arg(short = 'd', long = "max-depth")]
    pub max_depth: Option<usize>,

    /// Disable gitignore filtering
    #[arg(long = "no-gitignore")]
    pub no_gitignore: bool,

    /// Hide statistics dashboard
    #[arg(long = "no-stats")]
    pub no_stats: bool,

    /// Disable automatic recording
    #[arg(long = "no-record")]
    pub no_record: bool,

    /// Don't open the viewer automatically after a session
    #[arg(long = "no-open")]
    pub no_open: bool,

    /// Record to a specific file (default: auto-generated in recordings/)
    #[arg(short = 'r', long = "record")]
    pub record: Option<String>,

    /// Record file contents in addition to events
    #[arg(short = 'c', long = "content")]
    pub content: bool,

    /// Replay from a recorded JSON file
    #[arg(long = "replay")]
    pub replay: Option<String>,

    /// Replay speed multiplier
    #[arg(long = "replay-speed", default_value_t = 1.0)]
    pub replay_speed: f64,

    /// Open web replay viewer
    #[arg(long = "viewer")]
    pub viewer: bool,

    /// Generate a shareable command from a recording JSON file
    #[arg(long = "share")]
    pub share: Option<String>,

    /// Load a shared recording and open the viewer
    #[arg(long = "load")]
    pub load: Option<String>,

    /// Generate a recording from git commits. Accepts a commit hash, a range
    /// (e.g. abc123..def456), or a range to HEAD (e.g. abc123..)
    #[arg(long)]
    pub git: Option<String>,

    /// Disable watching git worktrees. By default, chronocode discovers
    /// worktrees via `git worktree list` and records changes in all of them.
    /// Worktree paths are always included even if they would be gitignored.
    #[arg(long = "no-worktrees")]
    pub no_worktrees: bool,
}
