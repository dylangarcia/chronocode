//! Generate a ChronoCode recording from git commit history.
//!
//! Supports:
//! - Single commit: `--git abc123` (diff from parent to that commit)
//! - Range: `--git abc123..def456` (all commits from abc123 to def456)
//! - Range to HEAD: `--git abc123..` (all commits from abc123 to HEAD)

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::state::{EventType, FileEvent};

/// A file entry from `git ls-tree`.
struct TreeEntry {
    path: String,
    size: u64,
}

/// A diff entry from `git diff-tree`.
struct DiffEntry {
    status: char,
    path: String,
}

/// The result of generating a recording from git history.
pub struct GitRecording {
    pub initial_state: Vec<Value>,
    pub events: Vec<FileEvent>,
    pub start_time: f64,
    pub commit_count: usize,
}

/// Parse the `--git` spec and generate a recording.
pub fn generate_recording(spec: &str, repo_path: &Path) -> Result<GitRecording> {
    // Verify we're in a git repo.
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo_path)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!("not a git repository: {}", repo_path.display());
    }

    // Parse the spec into a list of commits.
    let commits = resolve_commits(spec, repo_path)?;
    if commits.is_empty() {
        bail!("no commits found for spec: {}", spec);
    }

    eprintln!(
        "Generating recording from {} commit{}...",
        commits.len(),
        if commits.len() == 1 { "" } else { "s" }
    );

    // Get the timestamp of the first commit as the recording start time.
    let start_time = get_commit_timestamp(&commits[0], repo_path)?;

    // Build initial state from the tree at the parent of the first commit
    // (or empty if the first commit is the root commit).
    let initial_state = build_initial_state(&commits[0], repo_path)?;

    // Build events from each commit's diff.
    let mut events = Vec::new();
    for commit in &commits {
        let commit_time = get_commit_timestamp(commit, repo_path)?;
        let timestamp = commit_time - start_time;
        let diff_entries = get_commit_diff(commit, repo_path)?;

        for entry in diff_entries {
            let event_type = match entry.status {
                'A' => EventType::Created,
                'M' => EventType::Modified,
                'D' => EventType::Deleted,
                _ => continue,
            };

            let (size, loc) = if entry.status != 'D' {
                get_file_stats(&entry.path, commit, repo_path)
            } else {
                (0, 0)
            };

            events.push(FileEvent {
                timestamp,
                event_type,
                path: entry.path,
                size,
                is_dir: false,
                loc,
                content: None,
            });
        }
    }

    let commit_count = commits.len();
    Ok(GitRecording {
        initial_state,
        events,
        start_time,
        commit_count,
    })
}

/// Resolve a git spec into an ordered list of commit hashes.
fn resolve_commits(spec: &str, repo_path: &Path) -> Result<Vec<String>> {
    if spec.contains("..") {
        // Range: A..B or A..
        let parts: Vec<&str> = spec.splitn(2, "..").collect();
        let from = parts[0];
        let to = if parts[1].is_empty() {
            "HEAD"
        } else {
            parts[1]
        };

        // Resolve the full hashes.
        let from_hash = resolve_rev(from, repo_path)?;
        let to_hash = resolve_rev(to, repo_path)?;

        // Get all commits in the range, oldest first.
        let output = Command::new("git")
            .args([
                "log",
                "--format=%H",
                "--reverse",
                &format!("{}..{}", from_hash, to_hash),
            ])
            .current_dir(repo_path)
            .output()
            .context("failed to run git log")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git log failed: {}", stderr.trim());
        }

        let commits: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();

        if commits.is_empty() {
            bail!("no commits in range {}..{}", from, to);
        }

        Ok(commits)
    } else {
        // Single commit.
        let hash = resolve_rev(spec, repo_path)?;
        Ok(vec![hash])
    }
}

/// Resolve a rev (branch, tag, hash, HEAD, etc.) to a full commit hash.
fn resolve_rev(rev: &str, repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", rev])
        .current_dir(repo_path)
        .output()
        .context("failed to run git rev-parse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("cannot resolve '{}': {}", rev, stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the author timestamp (UNIX epoch seconds) of a commit.
fn get_commit_timestamp(hash: &str, repo_path: &Path) -> Result<f64> {
    let output = Command::new("git")
        .args(["log", "-1", "--format=%at", hash])
        .current_dir(repo_path)
        .output()
        .context("failed to get commit timestamp")?;

    let ts_str = String::from_utf8_lossy(&output.stdout);
    ts_str
        .trim()
        .parse::<f64>()
        .context("invalid commit timestamp")
}

/// Build the initial state from the tree at the parent of `commit`.
/// If the commit has no parent (root commit), returns an empty state.
fn build_initial_state(commit: &str, repo_path: &Path) -> Result<Vec<Value>> {
    // Check if this commit has a parent.
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("{}^", commit)])
        .current_dir(repo_path)
        .output()
        .context("failed to check parent commit")?;

    if !output.status.success() {
        // Root commit — no parent, empty initial state.
        return Ok(Vec::new());
    }

    let parent = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // List all files in the parent tree.
    let entries = list_tree(&parent, repo_path)?;

    // Also collect directory entries by scanning paths.
    let mut dirs = std::collections::HashSet::new();
    for entry in &entries {
        let path = Path::new(&entry.path);
        let mut current = path.parent();
        while let Some(dir) = current {
            if dir.as_os_str().is_empty() {
                break;
            }
            let dir_str = dir.to_string_lossy().to_string();
            if !dirs.insert(dir_str) {
                break; // Already seen this directory and all parents.
            }
            current = dir.parent();
        }
    }

    let mut state: Vec<Value> = Vec::new();

    // Add directory entries.
    for dir in &dirs {
        state.push(json!({
            "path": dir,
            "size": 0,
            "is_dir": true,
            "loc": 0,
        }));
    }

    // Add file entries.
    for entry in entries {
        let loc = count_lines_at_rev(&entry.path, &parent, repo_path);
        state.push(json!({
            "path": entry.path,
            "size": entry.size,
            "is_dir": false,
            "loc": loc,
        }));
    }

    Ok(state)
}

/// List all blob entries in the tree at `rev`.
fn list_tree(rev: &str, repo_path: &Path) -> Result<Vec<TreeEntry>> {
    let output = Command::new("git")
        .args(["ls-tree", "-r", "--long", rev])
        .current_dir(repo_path)
        .output()
        .context("failed to run git ls-tree")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git ls-tree failed: {}", stderr.trim());
    }

    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        // Format: <mode> <type> <hash> <size>\t<path>
        // e.g.:   100644 blob abc123 1234\tsrc/main.rs
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let parts: Vec<&str> = meta.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let obj_type = parts[1];
        if obj_type != "blob" {
            continue;
        }
        let size = parts[3].trim().parse::<u64>().unwrap_or(0);
        entries.push(TreeEntry {
            path: path.to_string(),
            size,
        });
    }

    Ok(entries)
}

/// Get the diff entries for a single commit (against its parent).
fn get_commit_diff(commit: &str, repo_path: &Path) -> Result<Vec<DiffEntry>> {
    let output = Command::new("git")
        .args(["diff-tree", "--no-commit-id", "-r", "--name-status", commit])
        .current_dir(repo_path)
        .output()
        .context("failed to run git diff-tree")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git diff-tree failed: {}", stderr.trim());
    }

    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: <status>\t<path>
        // For renames: R<score>\t<old_path>\t<new_path>
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }
        let status_str = parts[0];
        let status = status_str.chars().next().unwrap_or('?');

        match status {
            'A' | 'M' | 'D' => {
                entries.push(DiffEntry {
                    status,
                    path: parts[1].to_string(),
                });
            }
            'R' | 'C' => {
                // Rename/Copy: treat as delete old + create new.
                if parts.len() >= 3 {
                    entries.push(DiffEntry {
                        status: 'D',
                        path: parts[1].to_string(),
                    });
                    entries.push(DiffEntry {
                        status: 'A',
                        path: parts[2].to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(entries)
}

/// Get the size and LOC of a file at a specific commit.
fn get_file_stats(path: &str, commit: &str, repo_path: &Path) -> (u64, usize) {
    // Get file content to compute size and LOC.
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", commit, path)])
        .current_dir(repo_path)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let size = out.stdout.len() as u64;
            let loc = out.stdout.iter().filter(|&&b| b == b'\n').count();
            (size, loc)
        }
        _ => (0, 0),
    }
}

/// Count the number of lines in a file at a specific revision.
fn count_lines_at_rev(path: &str, rev: &str, repo_path: &Path) -> usize {
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", rev, path)])
        .current_dir(repo_path)
        .output();

    match output {
        Ok(out) if out.status.success() => out.stdout.iter().filter(|&&b| b == b'\n').count(),
        _ => 0,
    }
}
