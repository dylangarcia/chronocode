use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};

/// Parsed representation of a single gitignore rule.
struct Rule {
    /// Whether this rule is a negation (line started with `!`).
    is_negation: bool,
    /// Whether the original pattern had a trailing `/` (directory-only match).
    dir_only: bool,
    /// Compiled glob matcher for direct matches.
    matcher: GlobMatcher,
    /// Pre-compiled glob matcher for `pattern/**` (child/directory-content matches).
    child_matcher: GlobMatcher,
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
    /// Create a new parser rooted at `root_path`.
    ///
    /// Only loads the root-level `.gitignore` eagerly.  Nested `.gitignore`
    /// files are loaded on demand via [`load_gitignore_at`] as the scanner
    /// discovers them during the directory walk.
    pub fn new(root_path: &Path) -> Self {
        let mut parser = Self {
            root_path: root_path.to_path_buf(),
            patterns: HashMap::new(),
            rules: HashMap::new(),
        };
        // Eagerly load only the root .gitignore so top-level ignores
        // (e.g. `node_modules/`, `target/`) take effect immediately,
        // allowing the scanner to skip those subtrees entirely.
        let root_gitignore = root_path.join(".gitignore");
        if root_gitignore.is_file() {
            parser.load_gitignore_at(&root_gitignore);
        }
        parser
    }

    // ------------------------------------------------------------------
    // Loading
    // ------------------------------------------------------------------

    /// Load and compile a single `.gitignore` file.  Called by the scanner
    /// when it encounters a `.gitignore` during the directory walk.
    pub fn load_gitignore_at(&mut self, gitignore_path: &Path) {
        if !gitignore_path.is_file() {
            return;
        }
        let dir = gitignore_path
            .parent()
            .unwrap_or(&self.root_path)
            .to_path_buf();

        // Don't re-parse if we already have this directory's rules.
        if self.rules.contains_key(&dir) {
            return;
        }

        let raw_patterns = Self::parse_gitignore(gitignore_path);
        let compiled = raw_patterns
            .iter()
            .filter_map(|(pat, neg)| Self::compile_rule(pat, *neg))
            .collect();

        self.patterns.insert(dir.clone(), raw_patterns);
        self.rules.insert(dir, compiled);
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
    ///
    /// Pre-compiles both the direct matcher and the `pattern/**` child matcher
    /// so no glob compilation is needed at query time.
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

        // Build the glob expressions.
        //
        // * Anchored patterns are matched against the full relative path, so we
        //   use the pattern as-is.
        // * Un-anchored patterns can match in any sub-directory, so we prepend
        //   `**/`.
        let (glob_expr, child_glob_expr) = if anchored {
            (pat.clone(), format!("{pat}/**"))
        } else {
            (format!("**/{pat}"), format!("**/{pat}/**"))
        };

        let matcher = Glob::new(&glob_expr).ok()?.compile_matcher();
        let child_matcher = Glob::new(&child_glob_expr).ok()?.compile_matcher();

        Some(Rule {
            is_negation,
            dir_only,
            matcher,
            child_matcher,
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
    /// `is_dir` indicates whether the path is a directory (avoids a stat syscall;
    /// the caller typically already knows this from `walkdir::DirEntry`).
    ///
    /// The method walks from the repository root down towards `path`, applying
    /// each intermediate `.gitignore` in order.  Within a single `.gitignore`,
    /// later rules override earlier ones, and negation rules (`!pattern`) can
    /// un-ignore a previously ignored path.
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        // Build a canonical relative path (forward-slash separated) so that
        // glob matching works consistently across platforms.
        let rel = match path.strip_prefix(&self.root_path) {
            Ok(r) => r,
            Err(_) => return false,
        };

        // Build the relative path string without intermediate allocations by
        // writing directly into a single String.
        let mut rel_str = String::new();
        for (i, component) in rel.components().enumerate() {
            if i > 0 {
                rel_str.push('/');
            }
            rel_str.push_str(&component.as_os_str().to_string_lossy());
        }

        if rel_str.is_empty() {
            return false;
        }

        // Collect all applicable .gitignore directories.  These are every
        // ancestor of `path` (inclusive) that has a .gitignore, sorted from the
        // root towards the file so that closer gitignores take precedence by
        // being evaluated later.
        let mut applicable_dirs: Vec<&PathBuf> = self
            .rules
            .keys()
            .filter(|dir| path.starts_with(dir) || *dir == &self.root_path)
            .collect();

        applicable_dirs.sort_unstable();

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

            let mut local_rel_str = String::new();
            for (i, component) in local_rel.components().enumerate() {
                if i > 0 {
                    local_rel_str.push('/');
                }
                local_rel_str.push_str(&component.as_os_str().to_string_lossy());
            }

            if local_rel_str.is_empty() {
                continue;
            }

            for rule in rules {
                let target = &local_rel_str;

                // Check direct match.
                let direct_match = rule.matcher.is_match(target);

                // Check pre-compiled `pattern/**` to catch files *inside* an
                // ignored directory (e.g. `build/` should ignore
                // `build/output/a.bin`).
                let child_match = rule.child_matcher.is_match(target);

                // `dir_only` rules (trailing `/`) only match directories
                // directly, but they *do* match any file nested inside that
                // directory via the child_match path.
                let matched = if rule.dir_only && !is_dir {
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
        let mut parser = GitignoreParser::new(&dir);
        // Manually load the nested .gitignore (in production the scanner does this).
        parser.load_gitignore_at(&dir.join("src/.gitignore"));

        // build/ is ignored
        assert!(parser.is_ignored(&dir.join("build"), true));
        assert!(parser.is_ignored(&dir.join("build/output/result.bin"), false));

        // *.log is ignored
        assert!(parser.is_ignored(&dir.join("logs/debug.log"), false));

        // !important.log negates the *.log rule
        assert!(!parser.is_ignored(&dir.join("logs/important.log"), false));

        // Normal source files are not ignored
        assert!(!parser.is_ignored(&dir.join("src/main.rs"), false));

        // .tmp files inside src/ are ignored by nested .gitignore
        assert!(parser.is_ignored(&dir.join("src/temp.tmp"), false));

        teardown(&dir);
    }
}
