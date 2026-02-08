use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};

/// Parsed representation of a single gitignore rule.
struct Rule {
    /// The original pattern string (after stripping negation prefix and trailing slash).
    _pattern: String,
    /// Whether this rule is a negation (line started with `!`).
    is_negation: bool,
    /// Whether the original pattern had a trailing `/` (directory-only match).
    dir_only: bool,
    /// Whether the pattern is anchored (contains `/` so it must match from the
    /// .gitignore's directory root rather than any path component).
    _anchored: bool,
    /// Compiled glob matcher.
    matcher: GlobMatcher,
}

/// A gitignore parser that loads every `.gitignore` file found under a root
/// directory and can answer "is this path ignored?" queries.
pub struct GitignoreParser {
    root_path: PathBuf,
    /// Maps each directory that contains a `.gitignore` to its ordered list of
    /// `(pattern_string, is_negation)` pairs — exposed for introspection.
    pub patterns: HashMap<PathBuf, Vec<(String, bool)>>,
    /// Internal compiled rules keyed by the same directory.
    rules: HashMap<PathBuf, Vec<Rule>>,
}

impl GitignoreParser {
    /// Create a new parser rooted at `root_path`.  All `.gitignore` files under
    /// that directory are discovered and parsed immediately.
    pub fn new(root_path: &Path) -> Self {
        let mut parser = Self {
            root_path: root_path.to_path_buf(),
            patterns: HashMap::new(),
            rules: HashMap::new(),
        };
        parser.load_gitignores();
        parser
    }

    // ------------------------------------------------------------------
    // Loading
    // ------------------------------------------------------------------

    /// Recursively walk `root_path` and parse every `.gitignore` file found.
    fn load_gitignores(&mut self) {
        let walker = walkdir::WalkDir::new(&self.root_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in walker {
            if entry.file_type().is_file() && entry.file_name() == ".gitignore" {
                let gitignore_path = entry.path().to_path_buf();
                let dir = gitignore_path
                    .parent()
                    .unwrap_or(&self.root_path)
                    .to_path_buf();

                let raw_patterns = Self::parse_gitignore(&gitignore_path);
                let compiled = raw_patterns
                    .iter()
                    .filter_map(|(pat, neg)| Self::compile_rule(pat, *neg))
                    .collect();

                self.patterns.insert(dir.clone(), raw_patterns);
                self.rules.insert(dir, compiled);
            }
        }
    }

    // ------------------------------------------------------------------
    // Parsing
    // ------------------------------------------------------------------

    /// Parse a single `.gitignore` file into a list of `(pattern, is_negation)`
    /// pairs, preserving order.
    fn parse_gitignore(path: &Path) -> Vec<(String, bool)> {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let reader = std::io::BufReader::new(file);
        let mut results: Vec<(String, bool)> = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let trimmed = line.trim().to_string();

            // Skip blank lines and comments.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Detect negation.
            let (pattern, is_negation) = if let Some(rest) = trimmed.strip_prefix('!') {
                (rest.to_string(), true)
            } else {
                (trimmed, false)
            };

            if pattern.is_empty() {
                continue;
            }

            results.push((pattern, is_negation));
        }

        results
    }

    // ------------------------------------------------------------------
    // Compiling rules
    // ------------------------------------------------------------------

    /// Turn a raw `(pattern, is_negation)` pair into a compiled [`Rule`].
    fn compile_rule(pattern: &str, is_negation: bool) -> Option<Rule> {
        let mut pat = pattern.to_string();

        // Track and strip trailing `/` (directory-only match).
        let dir_only = pat.ends_with('/');
        if dir_only {
            pat = pat.trim_end_matches('/').to_string();
        }

        // Strip a single leading `/` — it anchors the pattern to the
        // .gitignore's directory but shouldn't be part of the glob.
        let had_leading_slash = pat.starts_with('/');
        if had_leading_slash {
            pat = pat[1..].to_string();
        }

        // A pattern is anchored when it contains a `/` (after stripping the
        // leading one) *or* had a leading `/`.
        let anchored = had_leading_slash || pat.contains('/');

        // Build the glob expression.
        //
        // * Anchored patterns are matched against the full relative path, so we
        //   use the pattern as-is.
        // * Un-anchored patterns can match in any sub-directory, so we prepend
        //   `**/`.
        let glob_expr = if anchored {
            // If the user wrote something like `build/` we want it to match
            // `build` **and** anything below it.  The glob `build` alone would
            // only match the directory entry itself, so we also try
            // `build/**`.  We handle this at match-time by checking both.
            pat.clone()
        } else {
            format!("**/{pat}")
        };

        let matcher = Glob::new(&glob_expr).ok()?.compile_matcher();

        Some(Rule {
            _pattern: pattern.to_string(),
            is_negation,
            dir_only,
            _anchored: anchored,
            matcher,
        })
    }

    // ------------------------------------------------------------------
    // Matching
    // ------------------------------------------------------------------

    /// Simple free-function that checks whether `rel_path` matches a gitignore
    /// `pattern`.  This uses glob-style matching and mirrors the logic encoded
    /// in `compile_rule` / `Rule::matcher` but is provided as a standalone
    /// helper for callers that only need a one-shot test.
    #[cfg(test)]
    pub fn match_pattern(rel_path: &str, pattern: &str) -> bool {
        let mut pat = pattern.to_string();

        // Strip trailing `/`.
        let _dir_only = pat.ends_with('/');
        if pat.ends_with('/') {
            pat = pat.trim_end_matches('/').to_string();
        }

        // Strip leading `/`.
        let had_leading_slash = pat.starts_with('/');
        if had_leading_slash {
            pat = pat[1..].to_string();
        }

        let anchored = had_leading_slash || pat.contains('/');

        let glob_expr = if anchored {
            pat.clone()
        } else {
            format!("**/{pat}")
        };

        // Try the base pattern.
        if let Ok(glob) = Glob::new(&glob_expr) {
            let m = glob.compile_matcher();
            if m.is_match(rel_path) {
                return true;
            }
        }

        // Also try `pattern/**` to handle directory contents.
        let dir_glob_expr = if anchored {
            format!("{pat}/**")
        } else {
            format!("**/{pat}/**")
        };

        if let Ok(glob) = Glob::new(&dir_glob_expr) {
            let m = glob.compile_matcher();
            if m.is_match(rel_path) {
                return true;
            }
        }

        false
    }

    // ------------------------------------------------------------------
    // Public query
    // ------------------------------------------------------------------

    /// Returns `true` if `path` should be ignored according to all applicable
    /// `.gitignore` rules.
    ///
    /// The method walks from the repository root down towards `path`, applying
    /// each intermediate `.gitignore` in order.  Within a single `.gitignore`,
    /// later rules override earlier ones, and negation rules (`!pattern`) can
    /// un-ignore a previously ignored path.
    pub fn is_ignored(&self, path: &Path) -> bool {
        // Build a canonical relative path (forward-slash separated) so that
        // glob matching works consistently across platforms.
        let rel = match path.strip_prefix(&self.root_path) {
            Ok(r) => r,
            Err(_) => return false,
        };

        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");

        if rel_str.is_empty() {
            return false;
        }

        let is_dir = path.is_dir();

        // Collect all applicable .gitignore directories.  These are every
        // ancestor of `path` (inclusive) that has a .gitignore, sorted from the
        // root towards the file so that closer gitignores take precedence by
        // being evaluated later.
        let mut applicable_dirs: Vec<&PathBuf> = self
            .rules
            .keys()
            .filter(|dir| path.starts_with(dir) || *dir == &self.root_path)
            .collect();

        applicable_dirs.sort();

        let mut ignored = false;

        for dir in applicable_dirs {
            let rules = match self.rules.get(dir) {
                Some(r) => r,
                None => continue,
            };

            // Compute the relative path *from this .gitignore's directory*.
            let local_rel = match path.strip_prefix(dir) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let local_rel_str = local_rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");

            if local_rel_str.is_empty() {
                continue;
            }

            for rule in rules {
                // For anchored patterns, match against the path relative to the
                // .gitignore's own directory.  For un-anchored patterns, the
                // glob already has a leading `**/` so either relative path will
                // work; we use the local one for consistency.
                let target = &local_rel_str;

                // Check direct match.
                let direct_match = rule.matcher.is_match(target);

                // Check `pattern/**` to catch files *inside* an ignored
                // directory (e.g. `build/` should ignore `build/output/a.bin`).
                let child_match = {
                    let dir_pattern = format!("{}/**", rule.matcher.glob().glob());
                    Glob::new(&dir_pattern)
                        .map(|g| g.compile_matcher().is_match(target))
                        .unwrap_or(false)
                };

                // `dir_only` rules (trailing `/`) only match directories
                // directly, but they *do* match any file nested inside that
                // directory via the child_match path.
                let matched = if rule.dir_only && !is_dir {
                    // For a non-directory path, a dir_only rule only applies
                    // if the path is *inside* the matched directory.
                    child_match
                } else {
                    direct_match || child_match
                };

                if matched {
                    ignored = !rule.is_negation;
                }
            }
        }

        ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a temporary directory tree with a `.gitignore` and some
    /// files, returning the root path.
    fn setup_temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("chronocode_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("build/output")).unwrap();
        fs::create_dir_all(dir.join("logs")).unwrap();

        // Root .gitignore
        fs::write(
            dir.join(".gitignore"),
            "# Build artifacts\nbuild/\n*.log\n!important.log\n",
        )
        .unwrap();

        // Nested .gitignore
        fs::write(dir.join("src/.gitignore"), "*.tmp\n").unwrap();

        // Create some files
        fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("src/temp.tmp"), "temp").unwrap();
        fs::write(dir.join("logs/debug.log"), "log").unwrap();
        fs::write(dir.join("logs/important.log"), "important").unwrap();
        fs::write(dir.join("build/output/result.bin"), "bin").unwrap();

        dir
    }

    fn teardown(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_parse_gitignore_skips_comments_and_blanks() {
        let dir = std::env::temp_dir().join("chronocode_parse_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".gitignore"), "# comment\n\n  \nfoo\n!bar\nbaz/\n").unwrap();

        let patterns = GitignoreParser::parse_gitignore(&dir.join(".gitignore"));
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0], ("foo".to_string(), false));
        assert_eq!(patterns[1], ("bar".to_string(), true));
        assert_eq!(patterns[2], ("baz/".to_string(), false));

        teardown(&dir);
    }

    #[test]
    fn test_match_pattern_simple_glob() {
        assert!(GitignoreParser::match_pattern("foo.log", "*.log"));
        assert!(GitignoreParser::match_pattern("a/b/foo.log", "*.log"));
        assert!(!GitignoreParser::match_pattern("foo.txt", "*.log"));
    }

    #[test]
    fn test_match_pattern_directory() {
        assert!(GitignoreParser::match_pattern("build", "build/"));
        assert!(GitignoreParser::match_pattern(
            "build/output/file.bin",
            "build/"
        ));
    }

    #[test]
    fn test_match_pattern_anchored() {
        assert!(GitignoreParser::match_pattern("src/main.rs", "src/main.rs"));
        assert!(!GitignoreParser::match_pattern(
            "other/src/main.rs",
            "/src/main.rs"
        ));
    }

    #[test]
    fn test_is_ignored_basic() {
        let dir = setup_temp_dir();
        let parser = GitignoreParser::new(&dir);

        // build/ is ignored
        assert!(parser.is_ignored(&dir.join("build")));
        assert!(parser.is_ignored(&dir.join("build/output/result.bin")));

        // *.log is ignored
        assert!(parser.is_ignored(&dir.join("logs/debug.log")));

        // !important.log negates the *.log rule
        assert!(!parser.is_ignored(&dir.join("logs/important.log")));

        // Normal source files are not ignored
        assert!(!parser.is_ignored(&dir.join("src/main.rs")));

        // .tmp files inside src/ are ignored by nested .gitignore
        assert!(parser.is_ignored(&dir.join("src/temp.tmp")));

        teardown(&dir);
    }
}
