use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use crate::gitignore::GitignoreParser;
use crate::recording::EventLogger;
use crate::state::{get_loc, ChangeSet, EventType, FileInfo};
use crate::statistics::StatisticsTracker;

/// Cached LOC entry: `(mtime, size, loc)`.
type LocCacheEntry = (f64, u64, usize);

/// Result of a background scan, returned via channel.
pub struct ScanResult {
    pub state: HashMap<PathBuf, FileInfo>,
    /// The gitignore parser, returned so it can be put back into the tracker
    /// (it may have been updated with newly-discovered nested .gitignore files).
    pub gitignore_parser: Option<GitignoreParser>,
}

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
    /// Worktree directories to scan in addition to the root.  These are
    /// absolute, canonical paths.  Worktrees that live under `root_path`
    /// are force-included (bypassing gitignore); worktrees outside are
    /// scanned separately and merged into the state.
    worktree_paths: Vec<PathBuf>,
    /// Pre-computed set of worktree canonical paths for O(1) lookups during
    /// the gitignore check.
    worktree_path_set: HashSet<PathBuf>,
    /// Whether the initial scan has completed.  LOC counting is deferred
    /// until after the first scan to avoid reading every file at startup.
    initial_scan_done: bool,
    /// Monotonically increasing counter bumped whenever `current_state`
    /// changes.  Used by the render cache to detect when it needs to rebuild.
    pub state_generation: u64,
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
            worktree_paths: Vec::new(),
            worktree_path_set: HashSet::new(),
            initial_scan_done: false,
            state_generation: 0,
        }
    }

    /// Register worktree directories to watch.  Paths should be absolute and
    /// canonical.
    pub fn set_worktree_paths(&mut self, paths: Vec<PathBuf>) {
        self.worktree_path_set = paths.iter().cloned().collect();
        self.worktree_paths = paths;
    }

    // ------------------------------------------------------------------
    // Directory scanning
    // ------------------------------------------------------------------

    /// Spawn the initial scan on a background thread.  Returns a receiver
    /// that will deliver the `ScanResult` once the scan completes.
    ///
    /// The gitignore parser is temporarily moved out of `self` and into the
    /// thread; it is returned inside `ScanResult` so the caller can put it
    /// back.
    pub fn spawn_background_scan(&mut self) -> mpsc::Receiver<ScanResult> {
        let (tx, rx) = mpsc::channel();

        let root_path = self.root_path.clone();
        let show_hidden = self.show_hidden;
        let use_gitignore = self.use_gitignore;
        let worktree_paths = self.worktree_paths.clone();
        let worktree_path_set = self.worktree_path_set.clone();
        let initial_scan_done = self.initial_scan_done;
        let gitignore_parser = self.gitignore_parser.take();

        std::thread::spawn(move || {
            // The background scan uses its own empty LOC cache.  On the
            // initial scan LOC counting is deferred anyway (all zeros), so
            // the cache isn't needed.
            let mut loc_cache = HashMap::new();
            let (state, gitignore_parser) = scan_directory_impl(
                &root_path,
                show_hidden,
                use_gitignore,
                &worktree_paths,
                &worktree_path_set,
                initial_scan_done,
                &mut loc_cache,
                gitignore_parser,
            );
            let _ = tx.send(ScanResult {
                state,
                gitignore_parser,
            });
        });

        rx
    }

    /// Apply the result of a background scan to the tracker state.
    pub fn apply_scan_result(&mut self, result: ScanResult) {
        self.gitignore_parser = result.gitignore_parser;
        self.previous_state = std::mem::take(&mut self.current_state);
        self.current_state = result.state;
        self.state_generation += 1;
        if !self.initial_scan_done {
            self.initial_scan_done = true;
        }
    }

    /// Perform a full directory scan synchronously.  Used for subsequent
    /// scans after the initial background scan has completed.
    pub fn scan_directory(&mut self, root_path: &Path) -> HashMap<PathBuf, FileInfo> {
        let gitignore_parser = self.gitignore_parser.take();
        let (state, parser_out) = scan_directory_impl(
            root_path,
            self.show_hidden,
            self.use_gitignore,
            &self.worktree_paths,
            &self.worktree_path_set,
            self.initial_scan_done,
            &mut self.loc_cache,
            gitignore_parser,
        );
        self.gitignore_parser = parser_out;
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
        self.state_generation += 1;

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
            added,
            modified,
            deleted,
        };

        // Forward events to logger and stats tracker.
        for path in &self.changes.added {
            let info = &self.current_state[path];
            let ext = path.extension().and_then(|e| e.to_str());
            if let Some(ref mut logger) = self.event_logger {
                logger.log_event(EventType::Created, path, info.size, info.is_dir, info.loc);
            }
            if let Some(ref mut tracker) = self.stats_tracker {
                tracker.record_event("created", info.size, info.is_dir, ext);
            }
        }

        for path in &self.changes.deleted {
            let info = &self.previous_state[path];
            let ext = path.extension().and_then(|e| e.to_str());
            if let Some(ref mut logger) = self.event_logger {
                logger.log_event(EventType::Deleted, path, info.size, info.is_dir, info.loc);
            }
            if let Some(ref mut tracker) = self.stats_tracker {
                tracker.record_event("deleted", info.size, info.is_dir, ext);
            }
        }

        for path in &self.changes.modified {
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
        for path in &self.changes.deleted {
            self.loc_cache.remove(path);
        }

        // After the first scan completes, enable LOC counting for subsequent
        // scans so only changed files are read.
        if !self.initial_scan_done {
            self.initial_scan_done = true;
        }
    }
}

// ======================================================================
// Standalone scan implementation (usable from both main and bg threads)
// ======================================================================

/// Returns `true` if any component of `path` (relative to `root`) starts with
/// a dot.
fn path_is_hidden(path: &Path, root: &Path) -> bool {
    let rel = match path.strip_prefix(root) {
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

/// Returns `true` if the first component of `path` relative to `root` is
/// `"recordings"`.
fn path_is_recordings(path: &Path, root: &Path) -> bool {
    let rel = match path.strip_prefix(root) {
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

/// Returns `true` if `path` falls under any path in the worktree set.
fn path_in_worktree(path: &Path, worktree_set: &HashSet<PathBuf>) -> bool {
    if worktree_set.is_empty() {
        return false;
    }
    let mut current = Some(path);
    while let Some(p) = current {
        if worktree_set.contains(p) {
            return true;
        }
        current = p.parent();
    }
    false
}

/// Core scan logic shared by both synchronous and background scan paths.
///
/// Returns `(state, gitignore_parser)`.
#[allow(clippy::too_many_arguments)]
fn scan_directory_impl(
    root_path: &Path,
    show_hidden: bool,
    use_gitignore: bool,
    worktree_paths: &[PathBuf],
    worktree_path_set: &HashSet<PathBuf>,
    initial_scan_done: bool,
    loc_cache: &mut HashMap<PathBuf, LocCacheEntry>,
    mut gitignore_parser: Option<GitignoreParser>,
) -> (HashMap<PathBuf, FileInfo>, Option<GitignoreParser>) {
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

    let mut walker = WalkDir::new(root_path).follow_links(false).into_iter();

    while let Some(entry_result) = walker.next() {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path().to_path_buf();

        if path == root_path {
            continue;
        }

        if entry.path_is_symlink() {
            continue;
        }

        let entry_is_dir = entry.file_type().is_dir();

        if !show_hidden && path_is_hidden(&path, root_path) {
            if entry_is_dir {
                walker.skip_current_dir();
            }
            continue;
        }

        if path_is_recordings(&path, root_path) {
            if entry_is_dir {
                walker.skip_current_dir();
            }
            continue;
        }

        // Incrementally load nested .gitignore files.
        if use_gitignore && entry.file_type().is_file() && entry.file_name() == ".gitignore" {
            if let Some(ref mut parser) = gitignore_parser {
                parser.load_gitignore_at(&path);
            }
        }

        // Skip gitignored paths; for ignored dirs skip the entire subtree.
        if use_gitignore && !path_in_worktree(&path, worktree_path_set) {
            if let Some(ref parser) = gitignore_parser {
                if parser.is_ignored(&path, entry_is_dir) {
                    if entry_is_dir {
                        walker.skip_current_dir();
                    }
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

            let loc = if !initial_scan_done {
                0
            } else if let Some(&(cached_mtime, cached_size, cached_loc)) = loc_cache.get(&path) {
                if cached_mtime == mtime && cached_size == size {
                    cached_loc
                } else {
                    let new_loc = get_loc(&path);
                    loc_cache.insert(path.clone(), (mtime, size, new_loc));
                    new_loc
                }
            } else {
                let new_loc = get_loc(&path);
                loc_cache.insert(path.clone(), (mtime, size, new_loc));
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

    // Scan external worktrees.
    for wt_path in worktree_paths {
        if wt_path.starts_with(root_path) {
            continue;
        }

        let wt_walker = WalkDir::new(wt_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());

        if let Ok(meta) = wt_path.metadata() {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            state.insert(
                wt_path.clone(),
                FileInfo {
                    size: 0,
                    modified: mtime,
                    is_dir: true,
                    loc: 0,
                },
            );
        }

        for entry in wt_walker {
            let path = entry.path().to_path_buf();
            if path == *wt_path {
                continue;
            }
            if entry.path_is_symlink() {
                continue;
            }
            if !show_hidden {
                let rel = path.strip_prefix(wt_path).unwrap_or(&path);
                let hidden = rel
                    .components()
                    .any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with('.')));
                if hidden {
                    continue;
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
                let loc = if !initial_scan_done {
                    0
                } else if let Some(&(cached_mtime, cached_size, cached_loc)) = loc_cache.get(&path)
                {
                    if cached_mtime == mtime && cached_size == size {
                        cached_loc
                    } else {
                        let new_loc = get_loc(&path);
                        loc_cache.insert(path.clone(), (mtime, size, new_loc));
                        new_loc
                    }
                } else {
                    let new_loc = get_loc(&path);
                    loc_cache.insert(path.clone(), (mtime, size, new_loc));
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
    }

    (state, gitignore_parser)
}
