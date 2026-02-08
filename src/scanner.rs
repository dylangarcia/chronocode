use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use crate::gitignore::GitignoreParser;
use crate::recording::EventLogger;
use crate::state::{get_loc, ChangeSet, EventType, FileInfo};
use crate::statistics::StatisticsTracker;

/// Cached LOC entry: `(mtime, size, loc)`.
type LocCacheEntry = (f64, u64, usize);

/// Tracks directory state across scans and detects file-level changes.
pub struct ChangeTracker {
    pub previous_state: HashMap<PathBuf, FileInfo>,
    pub current_state: HashMap<PathBuf, FileInfo>,
    pub changes: ChangeSet,
    root_path: PathBuf,
    use_gitignore: bool,
    show_hidden: bool,
    gitignore_parser: Option<GitignoreParser>,
    pub event_logger: Option<EventLogger>,
    pub stats_tracker: Option<StatisticsTracker>,
    /// Cache of LOC counts keyed by path.  Only recount when mtime or size
    /// changes compared to the cached values.
    loc_cache: HashMap<PathBuf, LocCacheEntry>,
}

impl ChangeTracker {
    /// Create a new `ChangeTracker` for the given root directory.
    ///
    /// If `use_gitignore` is `true`, a [`GitignoreParser`] is created and used
    /// to skip ignored paths during scans.
    pub fn new(
        root_path: PathBuf,
        use_gitignore: bool,
        show_hidden: bool,
        event_logger: Option<EventLogger>,
        stats_tracker: Option<StatisticsTracker>,
    ) -> Self {
        let gitignore_parser = if use_gitignore {
            Some(GitignoreParser::new(&root_path))
        } else {
            None
        };

        Self {
            previous_state: HashMap::new(),
            current_state: HashMap::new(),
            changes: ChangeSet::default(),
            root_path,
            use_gitignore,
            show_hidden,
            gitignore_parser,
            event_logger,
            stats_tracker,
            loc_cache: HashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Path filtering helpers
    // ------------------------------------------------------------------

    /// Returns `true` if any component of `path` (relative to the root)
    /// starts with a dot, indicating a hidden file or directory.
    fn is_hidden_path(&self, path: &Path) -> bool {
        let rel = match path.strip_prefix(&self.root_path) {
            Ok(r) => r,
            Err(_) => return false,
        };

        for component in rel.components() {
            if let Some(s) = component.as_os_str().to_str() {
                if s.starts_with('.') {
                    return true;
                }
            }
        }

        false
    }

    /// Returns `true` if the first component of `path` relative to the root
    /// is `"recordings"`.
    fn is_recordings_path(&self, path: &Path) -> bool {
        let rel = match path.strip_prefix(&self.root_path) {
            Ok(r) => r,
            Err(_) => return false,
        };

        if let Some(first) = rel.components().next() {
            if let Some(s) = first.as_os_str().to_str() {
                return s == "recordings";
            }
        }

        false
    }

    // ------------------------------------------------------------------
    // Directory scanning
    // ------------------------------------------------------------------

    /// Perform a full directory scan rooted at `root_path` and return a map
    /// of every discovered path to its [`FileInfo`].
    pub fn scan_directory(&mut self, root_path: &Path) -> HashMap<PathBuf, FileInfo> {
        let mut state: HashMap<PathBuf, FileInfo> = HashMap::new();

        // Add the root directory itself.
        if let Ok(meta) = root_path.metadata() {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            state.insert(
                root_path.to_path_buf(),
                FileInfo {
                    size: 0,
                    modified: mtime,
                    is_dir: true,
                    loc: 0,
                },
            );
        }

        let walker = WalkDir::new(root_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in walker {
            let path = entry.path().to_path_buf();

            // Skip the root itself (already added above).
            if path == root_path {
                continue;
            }

            // Skip symlinks.
            if entry.path_is_symlink() {
                continue;
            }

            // Skip hidden paths unless configured to show them.
            if !self.show_hidden && self.is_hidden_path(&path) {
                continue;
            }

            // Skip the recordings directory.
            if self.is_recordings_path(&path) {
                continue;
            }

            // Skip gitignored paths.
            if self.use_gitignore {
                if let Some(ref parser) = self.gitignore_parser {
                    if parser.is_ignored(&path) {
                        continue;
                    }
                }
            }

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            if meta.is_file() {
                let size = meta.len();

                // Use cached LOC count if mtime and size haven't changed.
                let loc = if let Some(&(cached_mtime, cached_size, cached_loc)) =
                    self.loc_cache.get(&path)
                {
                    if cached_mtime == mtime && cached_size == size {
                        cached_loc
                    } else {
                        let new_loc = get_loc(&path);
                        self.loc_cache.insert(path.clone(), (mtime, size, new_loc));
                        new_loc
                    }
                } else {
                    let new_loc = get_loc(&path);
                    self.loc_cache.insert(path.clone(), (mtime, size, new_loc));
                    new_loc
                };

                state.insert(
                    path,
                    FileInfo {
                        size,
                        modified: mtime,
                        is_dir: false,
                        loc,
                    },
                );
            } else if meta.is_dir() {
                state.insert(
                    path,
                    FileInfo {
                        size: 0,
                        modified: mtime,
                        is_dir: true,
                        loc: 0,
                    },
                );
            }
        }

        state
    }

    // ------------------------------------------------------------------
    // State update & change detection
    // ------------------------------------------------------------------

    /// Re-scan the directory, diff against the previous state, and populate
    /// `self.changes` with any additions, deletions, or modifications.
    ///
    /// Events are also forwarded to the optional [`EventLogger`] and
    /// [`StatisticsTracker`].
    pub fn update(&mut self, root_path: &Path) {
        // Rotate states.
        self.previous_state = std::mem::take(&mut self.current_state);
        self.current_state = self.scan_directory(root_path);

        // Compute change sets using key-set operations.
        let previous_keys: std::collections::HashSet<&PathBuf> =
            self.previous_state.keys().collect();
        let current_keys: std::collections::HashSet<&PathBuf> = self.current_state.keys().collect();

        let added: std::collections::HashSet<PathBuf> = current_keys
            .difference(&previous_keys)
            .map(|p| (*p).clone())
            .collect();

        let deleted: std::collections::HashSet<PathBuf> = previous_keys
            .difference(&current_keys)
            .map(|p| (*p).clone())
            .collect();

        let modified: std::collections::HashSet<PathBuf> = current_keys
            .intersection(&previous_keys)
            .filter(|p| {
                let prev = &self.previous_state[**p];
                let curr = &self.current_state[**p];
                prev.size != curr.size || prev.modified != curr.modified
            })
            .map(|p| (*p).clone())
            .collect();

        self.changes = ChangeSet {
            added: added.clone(),
            modified: modified.clone(),
            deleted: deleted.clone(),
        };

        // Forward events to logger and stats tracker.
        for path in &added {
            let info = &self.current_state[path];
            let ext = path.extension().and_then(|e| e.to_str());
            if let Some(ref mut logger) = self.event_logger {
                logger.log_event(EventType::Created, path, info.size, info.is_dir, info.loc);
            }
            if let Some(ref mut tracker) = self.stats_tracker {
                tracker.record_event("created", info.size, info.is_dir, ext);
            }
        }

        for path in &deleted {
            let info = &self.previous_state[path];
            let ext = path.extension().and_then(|e| e.to_str());
            if let Some(ref mut logger) = self.event_logger {
                logger.log_event(EventType::Deleted, path, info.size, info.is_dir, info.loc);
            }
            if let Some(ref mut tracker) = self.stats_tracker {
                tracker.record_event("deleted", info.size, info.is_dir, ext);
            }
        }

        for path in &modified {
            let info = &self.current_state[path];
            let ext = path.extension().and_then(|e| e.to_str());
            if let Some(ref mut logger) = self.event_logger {
                logger.log_event(EventType::Modified, path, info.size, info.is_dir, info.loc);
            }
            if let Some(ref mut tracker) = self.stats_tracker {
                tracker.record_event("modified", info.size, info.is_dir, ext);
            }
        }

        // Evict deleted paths from the LOC cache.
        for path in &deleted {
            self.loc_cache.remove(path);
        }
    }
}
