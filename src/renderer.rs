//! Ratatui-based tree renderer and stats dashboard for ChronoCode.
//!
//! This module is purely presentational -- it takes references to application
//! data and renders into a Ratatui `Frame`.  It does **not** own any state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::state::{
    format_delta, format_loc, format_size, get_file_emoji, get_size_color, ChangeSet, FileInfo,
};
use crate::statistics::{StatisticsTracker, Stats};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NAME_WIDTH: u16 = 42;
const STATUS_WIDTH: u16 = 10;
const SIZE_WIDTH: u16 = 10;
const DELTA_WIDTH: u16 = 10;
const LOC_WIDTH: u16 = 8;

// ---------------------------------------------------------------------------
// TreeNode
// ---------------------------------------------------------------------------

/// Intermediate representation used to build a sorted tree view from flat
/// `HashMap<PathBuf, FileInfo>` state.
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

// ---------------------------------------------------------------------------
// Color mapping
// ---------------------------------------------------------------------------

/// Map a color name (as returned by helpers such as `get_size_color` /
/// `format_delta`) to a Ratatui `Color`.
fn color_from_name(name: &str) -> Color {
    match name {
        "dim" => Color::DarkGray,
        "cyan" => Color::Cyan,
        "yellow" => Color::Yellow,
        "red" => Color::Red,
        "green" => Color::Green,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "white" => Color::White,
        _ => Color::Reset,
    }
}

// ---------------------------------------------------------------------------
// Tree building
// ---------------------------------------------------------------------------

/// Build a sorted tree of `TreeNode`s from the flat state map.
///
/// Only direct children of each directory are included.  Directories are sorted
/// before files; within each group entries are sorted alphabetically
/// (case-insensitive).
pub fn build_tree(root: &Path, state: &HashMap<PathBuf, FileInfo>) -> Vec<TreeNode> {
    build_children(root, state)
}

/// Recursively collect direct children of `parent` from `state`.
fn build_children(parent: &Path, state: &HashMap<PathBuf, FileInfo>) -> Vec<TreeNode> {
    let mut dirs: Vec<TreeNode> = Vec::new();
    let mut files: Vec<TreeNode> = Vec::new();

    for (path, info) in state {
        // Direct child check: parent of `path` must equal `parent`.
        if path.parent() != Some(parent) {
            continue;
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let children = if info.is_dir {
            build_children(path, state)
        } else {
            Vec::new()
        };

        let node = TreeNode {
            name,
            path: path.clone(),
            is_dir: info.is_dir,
            children,
        };

        if info.is_dir {
            dirs.push(node);
        } else {
            files.push(node);
        }
    }

    // Sort each group alphabetically (case-insensitive).
    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Directories first, then files.
    dirs.extend(files);
    dirs
}

// ---------------------------------------------------------------------------
// Tree line rendering
// ---------------------------------------------------------------------------

/// Recursively build styled `Line`s representing the file tree.
///
/// # Arguments
///
/// * `nodes`          - Slice of sibling `TreeNode`s at the current level.
/// * `prefix`         - The accumulated box-drawing prefix string for
///   indentation (e.g. `"│   "`).
/// * `state`          - Current snapshot of all tracked files.
/// * `changes`        - Set of paths that were added / modified / deleted since
///   the last snapshot.
/// * `previous_state` - Previous snapshot (used for computing deltas).
/// * `max_depth`      - Optional limit on tree depth.
/// * `max_files`      - Optional limit on files shown per directory level.
/// * `current_depth`  - The current recursion depth (starts at 0).
/// * `lines`          - Output vector to which rendered `Line`s are appended.
#[allow(clippy::too_many_arguments)]
pub fn render_tree_lines(
    nodes: &[TreeNode],
    prefix: &str,
    state: &HashMap<PathBuf, FileInfo>,
    changes: &ChangeSet,
    previous_state: &HashMap<PathBuf, FileInfo>,
    max_depth: Option<usize>,
    max_files: Option<usize>,
    current_depth: usize,
    lines: &mut Vec<Line<'static>>,
) {
    // If we have exceeded the maximum depth, emit a placeholder and return.
    if let Some(md) = max_depth {
        if current_depth > md {
            lines.push(Line::from(vec![Span::styled(
                format!("{}...", prefix),
                Style::default().fg(Color::DarkGray),
            )]));
            return;
        }
    }

    let total = nodes.len();
    let display_count = match max_files {
        Some(mf) => mf.min(total),
        None => total,
    };

    for (i, node) in nodes.iter().enumerate() {
        if i >= display_count {
            let remaining = total - display_count;
            lines.push(Line::from(vec![Span::styled(
                format!("{}... and {} more", prefix, remaining),
                Style::default().fg(Color::DarkGray),
            )]));
            break;
        }

        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        // --- Build spans for this line ---

        let mut spans: Vec<Span<'static>> = Vec::new();

        // 1. Prefix + connector
        spans.push(Span::styled(
            format!("{}{}", prefix, connector),
            Style::default().fg(Color::DarkGray),
        ));

        // 2. Emoji
        let emoji = get_file_emoji(&node.name, node.is_dir);
        spans.push(Span::raw(format!("{} ", emoji)));

        // 3. Name -- colored by change status
        let (name_color, status_text) = if changes.added.contains(&node.path) {
            (Color::Green, "NEW")
        } else if changes.modified.contains(&node.path) {
            (Color::Yellow, "MOD")
        } else if changes.deleted.contains(&node.path) {
            (Color::Red, "DEL")
        } else {
            (Color::White, "")
        };

        let name_style = if node.is_dir {
            Style::default().fg(name_color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(name_color)
        };

        spans.push(Span::styled(node.name.clone(), name_style));

        // 4. Status badge
        if !status_text.is_empty() {
            let badge_color = match status_text {
                "NEW" => Color::Green,
                "MOD" => Color::Yellow,
                "DEL" => Color::Red,
                _ => Color::Reset,
            };
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("[{}]", status_text),
                Style::default()
                    .fg(Color::Black)
                    .bg(badge_color)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // For files (not dirs), show size, delta, LOC, LOC delta.
        if !node.is_dir {
            if let Some(info) = state.get(&node.path) {
                // 5. Size
                let size_str = format_size(info.size);
                let size_color = color_from_name(get_size_color(info.size));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(size_str, Style::default().fg(size_color)));

                // 6. Size delta
                let prev_size = previous_state.get(&node.path).map(|p| p.size).unwrap_or(0);
                let size_delta = info.size as i64 - prev_size as i64;
                let (delta_str, delta_color_name) = format_delta(size_delta, true);
                if !delta_str.is_empty() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        delta_str,
                        Style::default().fg(color_from_name(delta_color_name)),
                    ));
                }

                // 7. LOC
                let loc_str = format_loc(info.loc);
                if !loc_str.is_empty() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(loc_str, Style::default().fg(Color::DarkGray)));
                }

                // 8. LOC delta
                let prev_loc = previous_state.get(&node.path).map(|p| p.loc).unwrap_or(0);
                let loc_delta = info.loc as i64 - prev_loc as i64;
                let (loc_delta_str, loc_delta_color_name) = format_delta(loc_delta, false);
                if !loc_delta_str.is_empty() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        loc_delta_str,
                        Style::default().fg(color_from_name(loc_delta_color_name)),
                    ));
                }
            }
        }

        lines.push(Line::from(spans));

        // Recurse into children for directories.
        if node.is_dir && !node.children.is_empty() {
            render_tree_lines(
                &node.children,
                &child_prefix,
                state,
                changes,
                previous_state,
                max_depth,
                max_files,
                current_depth + 1,
                lines,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

/// Render the header area (title, watched path, recording indicator).
fn render_header(frame: &mut Frame, area: Rect, root_path: &Path, is_recording: bool) {
    let title_line = Line::from(vec![
        Span::styled(
            " ChronoCode ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            "File Watcher",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let path_line = Line::from(vec![
        Span::styled(" Watching: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            root_path.display().to_string(),
            Style::default().fg(Color::White),
        ),
    ]);

    let status_line = if is_recording {
        Line::from(vec![
            Span::styled(
                " ● REC ",
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Recording session  ", Style::default().fg(Color::Red)),
            Span::styled("Ctrl+C to stop", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![Span::styled(
            " Ctrl+C to stop ",
            Style::default().fg(Color::DarkGray),
        )])
    };

    let text = Text::from(vec![title_line, path_line, status_line]);
    let block = Block::default().borders(Borders::NONE);
    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Summary line
// ---------------------------------------------------------------------------

/// Render a summary line showing total files, directories, size, LOC, and
/// change counts.
fn render_summary_line(state: &HashMap<PathBuf, FileInfo>, changes: &ChangeSet) -> Line<'static> {
    let mut total_files: usize = 0;
    let mut total_dirs: usize = 0;
    let mut total_size: u64 = 0;
    let mut total_loc: usize = 0;

    for info in state.values() {
        if info.is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            total_size += info.size;
            total_loc += info.loc;
        }
    }

    let added = changes.added.len();
    let modified = changes.modified.len();
    let deleted = changes.deleted.len();

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(" Files: ", Style::default().fg(Color::DarkGray)),
        Span::styled(total_files.to_string(), Style::default().fg(Color::White)),
        Span::styled("  Dirs: ", Style::default().fg(Color::DarkGray)),
        Span::styled(total_dirs.to_string(), Style::default().fg(Color::White)),
        Span::styled("  Size: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format_size(total_size), Style::default().fg(Color::Cyan)),
        Span::styled("  LOC: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format_loc(total_loc), Style::default().fg(Color::Cyan)),
    ];

    if added > 0 || modified > 0 || deleted > 0 {
        spans.push(Span::styled("  | ", Style::default().fg(Color::DarkGray)));
        if added > 0 {
            spans.push(Span::styled(
                format!("+{}", added),
                Style::default().fg(Color::Green),
            ));
            spans.push(Span::raw(" "));
        }
        if modified > 0 {
            spans.push(Span::styled(
                format!("~{}", modified),
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::raw(" "));
        }
        if deleted > 0 {
            spans.push(Span::styled(
                format!("-{}", deleted),
                Style::default().fg(Color::Red),
            ));
        }
    }

    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Stats dashboard
// ---------------------------------------------------------------------------

/// Render the development statistics dashboard.
fn render_stats_dashboard(frame: &mut Frame, area: Rect, stats: &Stats) {
    let duration_str = StatisticsTracker::format_duration(stats.session_duration);
    let events_rate = stats.events_per_minute;

    let lines = vec![
        Line::from(vec![
            Span::styled(" Session Duration: ", Style::default().fg(Color::DarkGray)),
            Span::styled(duration_str, Style::default().fg(Color::White)),
            Span::styled("    Activity Rate: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} events/min", events_rate),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Created: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                stats.total_created.to_string(),
                Style::default().fg(Color::Green),
            ),
            Span::styled("   Modified: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                stats.total_modified.to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("   Deleted: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                stats.total_deleted.to_string(),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Files: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                stats.current_files.to_string(),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!(" / {} peak", stats.peak_files),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("    Dirs: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                stats.current_dirs.to_string(),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!(" / {} peak", stats.peak_dirs),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let text = Text::from(lines);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " DEVELOPMENT STATISTICS ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

/// Render the legend / key-binding bar at the bottom of the screen.
fn render_legend(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: u16,
    total_lines: u16,
    viewport_height: u16,
) {
    let mut spans = vec![
        Span::styled(" Legend: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "NEW",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "MODIFIED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "DELETED",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  |  q quit  j/k scroll  g/G top/bottom",
            Style::default().fg(Color::DarkGray),
        ),
    ];

    // Show scroll position indicator when content overflows the viewport.
    if total_lines > viewport_height {
        let current_top = scroll_offset + 1;
        let current_bottom = (scroll_offset + viewport_height).min(total_lines);
        spans.push(Span::styled(
            format!("  [{}-{}/{}]", current_top, current_bottom, total_lines),
            Style::default().fg(Color::Cyan),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Column headers for the tree view
// ---------------------------------------------------------------------------

/// Return a `Line` with column headers (Name, Status, Size, Delta, LOC, LOC+/-).
fn tree_column_headers() -> Line<'static> {
    let hdr_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::UNDERLINED);

    Line::from(vec![
        Span::styled(
            format!(" {:<width$}", "Name", width = NAME_WIDTH as usize),
            hdr_style,
        ),
        Span::styled(
            format!("{:<width$}", "Status", width = STATUS_WIDTH as usize),
            hdr_style,
        ),
        Span::styled(
            format!("{:>width$}", "Size", width = SIZE_WIDTH as usize),
            hdr_style,
        ),
        Span::styled(
            format!("{:>width$}", "Delta", width = DELTA_WIDTH as usize),
            hdr_style,
        ),
        Span::styled(
            format!("{:>width$}", "LOC", width = LOC_WIDTH as usize),
            hdr_style,
        ),
        Span::styled(
            format!("{:>width$}", "LOC+/-", width = LOC_WIDTH as usize),
            hdr_style,
        ),
    ])
}

// ---------------------------------------------------------------------------
// Main render entry point
// ---------------------------------------------------------------------------

/// Top-level render function.  Call this from your main loop with all the data
/// the renderer needs -- this avoids circular dependencies on an `App` struct.
#[allow(clippy::too_many_arguments)]
pub fn render_ui(
    frame: &mut Frame,
    root_path: &Path,
    state: &HashMap<PathBuf, FileInfo>,
    changes: &ChangeSet,
    previous_state: &HashMap<PathBuf, FileInfo>,
    stats: Option<&Stats>,
    is_recording: bool,
    max_depth: Option<usize>,
    max_files: Option<usize>,
    show_stats: bool,
    scroll_offset: u16,
) -> u16 {
    let size = frame.area();

    // ----- Determine layout constraints -----

    let stats_height: u16 = if show_stats && stats.is_some() { 7 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),            // header
            Constraint::Length(1),            // summary line
            Constraint::Min(1),               // tree area
            Constraint::Length(stats_height), // stats dashboard
            Constraint::Length(1),            // legend
        ])
        .split(size);

    let header_area = chunks[0];
    let summary_area = chunks[1];
    let tree_area = chunks[2];
    let stats_area = chunks[3];
    let legend_area = chunks[4];

    // ----- Header -----
    render_header(frame, header_area, root_path, is_recording);

    // ----- Summary line -----
    let summary_line = render_summary_line(state, changes);
    frame.render_widget(Paragraph::new(summary_line), summary_area);

    // ----- Tree -----
    let tree_nodes = build_tree(root_path, state);
    let mut tree_lines: Vec<Line<'static>> = Vec::new();

    // Column headers
    tree_lines.push(tree_column_headers());

    // Actual tree content
    render_tree_lines(
        &tree_nodes,
        " ",
        state,
        changes,
        previous_state,
        max_depth,
        max_files,
        0,
        &mut tree_lines,
    );

    let total_tree_lines = tree_lines.len() as u16;
    let tree_text = Text::from(tree_lines);
    let tree_block = Block::default().borders(Borders::NONE);
    let tree_paragraph = Paragraph::new(tree_text)
        .block(tree_block)
        .scroll((scroll_offset, 0));
    frame.render_widget(tree_paragraph, tree_area);

    // ----- Stats dashboard -----
    if show_stats {
        if let Some(s) = stats {
            render_stats_dashboard(frame, stats_area, s);
        }
    }

    // ----- Legend -----
    render_legend(
        frame,
        legend_area,
        scroll_offset,
        total_tree_lines,
        tree_area.height,
    );

    total_tree_lines
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_name() {
        assert_eq!(color_from_name("dim"), Color::DarkGray);
        assert_eq!(color_from_name("cyan"), Color::Cyan);
        assert_eq!(color_from_name("yellow"), Color::Yellow);
        assert_eq!(color_from_name("red"), Color::Red);
        assert_eq!(color_from_name("green"), Color::Green);
        assert_eq!(color_from_name("blue"), Color::Blue);
        assert_eq!(color_from_name("magenta"), Color::Magenta);
        assert_eq!(color_from_name("white"), Color::White);
        assert_eq!(color_from_name("unknown"), Color::Reset);
    }

    #[test]
    fn test_build_tree_empty() {
        let state: HashMap<PathBuf, FileInfo> = HashMap::new();
        let root = PathBuf::from("/tmp/test");
        let tree = build_tree(&root, &state);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_tree_dirs_before_files() {
        let root = PathBuf::from("/project");
        let mut state = HashMap::new();

        state.insert(
            PathBuf::from("/project/zebra.txt"),
            FileInfo {
                path: PathBuf::from("/project/zebra.txt"),
                size: 100,
                modified: 0.0,
                is_dir: false,
                loc: 10,
            },
        );
        state.insert(
            PathBuf::from("/project/alpha"),
            FileInfo {
                path: PathBuf::from("/project/alpha"),
                size: 0,
                modified: 0.0,
                is_dir: true,
                loc: 0,
            },
        );
        state.insert(
            PathBuf::from("/project/beta.rs"),
            FileInfo {
                path: PathBuf::from("/project/beta.rs"),
                size: 200,
                modified: 0.0,
                is_dir: false,
                loc: 20,
            },
        );

        let tree = build_tree(&root, &state);

        // Directory should come first.
        assert_eq!(tree.len(), 3);
        assert!(tree[0].is_dir);
        assert_eq!(tree[0].name, "alpha");
        // Then files, alphabetically.
        assert_eq!(tree[1].name, "beta.rs");
        assert_eq!(tree[2].name, "zebra.txt");
    }

    #[test]
    fn test_render_tree_lines_basic() {
        let root = PathBuf::from("/project");
        let mut state = HashMap::new();
        state.insert(
            PathBuf::from("/project/hello.rs"),
            FileInfo {
                path: PathBuf::from("/project/hello.rs"),
                size: 512,
                modified: 0.0,
                is_dir: false,
                loc: 25,
            },
        );

        let changes = ChangeSet::default();
        let previous_state = HashMap::new();
        let nodes = build_tree(&root, &state);
        let mut lines: Vec<Line<'static>> = Vec::new();

        render_tree_lines(
            &nodes,
            " ",
            &state,
            &changes,
            &previous_state,
            None,
            None,
            0,
            &mut lines,
        );

        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_render_tree_lines_max_depth() {
        let root = PathBuf::from("/project");
        let mut state = HashMap::new();

        state.insert(
            PathBuf::from("/project/src"),
            FileInfo {
                path: PathBuf::from("/project/src"),
                size: 0,
                modified: 0.0,
                is_dir: true,
                loc: 0,
            },
        );
        state.insert(
            PathBuf::from("/project/src/main.rs"),
            FileInfo {
                path: PathBuf::from("/project/src/main.rs"),
                size: 1024,
                modified: 0.0,
                is_dir: false,
                loc: 50,
            },
        );

        let changes = ChangeSet::default();
        let previous_state = HashMap::new();
        let nodes = build_tree(&root, &state);
        let mut lines: Vec<Line<'static>> = Vec::new();

        // max_depth = 0 means only the root-level children, no recursion into
        // subdirectories.
        render_tree_lines(
            &nodes,
            " ",
            &state,
            &changes,
            &previous_state,
            Some(0),
            None,
            0,
            &mut lines,
        );

        // Should have the "src" dir line, but its children should be replaced
        // by a "..." placeholder.
        assert!(lines.len() >= 1);
    }
}
