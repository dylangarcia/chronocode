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
use unicode_width::UnicodeWidthStr;

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
    // Single-pass: group all entries by their parent directory.
    let mut children_map: HashMap<PathBuf, Vec<(PathBuf, bool)>> = HashMap::new();

    for (path, info) in state {
        if let Some(parent) = path.parent() {
            children_map
                .entry(parent.to_path_buf())
                .or_default()
                .push((path.clone(), info.is_dir));
        }
    }

    build_from_map(root, &children_map)
}

/// Recursively build tree nodes from the pre-computed children map.
fn build_from_map(
    parent: &Path,
    children_map: &HashMap<PathBuf, Vec<(PathBuf, bool)>>,
) -> Vec<TreeNode> {
    let Some(entries) = children_map.get(parent) else {
        return Vec::new();
    };

    let mut dirs: Vec<TreeNode> = Vec::new();
    let mut files: Vec<TreeNode> = Vec::new();

    for (path, is_dir) in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let children = if *is_dir {
            build_from_map(path, children_map)
        } else {
            Vec::new()
        };

        let node = TreeNode {
            name,
            path: path.clone(),
            is_dir: *is_dir,
            children,
        };

        if *is_dir {
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
// Tree filtering
// ---------------------------------------------------------------------------

/// Recursively filter tree nodes, keeping any node whose name matches the query
/// (case-insensitive) OR that has a descendant that matches.  Parent directories
/// of matching files are kept so the tree structure remains intact.
pub fn filter_tree(nodes: &[TreeNode], query: &str) -> Vec<TreeNode> {
    let query_lower = query.to_lowercase();
    nodes
        .iter()
        .filter_map(|node| filter_node(node, &query_lower))
        .collect()
}

/// Filter a single node: returns `Some(filtered_node)` if this node or any
/// descendant matches; `None` otherwise.
fn filter_node(node: &TreeNode, query_lower: &str) -> Option<TreeNode> {
    let name_matches = node.name.to_lowercase().contains(query_lower);

    if node.is_dir {
        let filtered_children: Vec<TreeNode> = node
            .children
            .iter()
            .filter_map(|child| filter_node(child, query_lower))
            .collect();

        if name_matches || !filtered_children.is_empty() {
            Some(TreeNode {
                name: node.name.clone(),
                path: node.path.clone(),
                is_dir: node.is_dir,
                children: filtered_children,
            })
        } else {
            None
        }
    } else if name_matches {
        Some(TreeNode {
            name: node.name.clone(),
            path: node.path.clone(),
            is_dir: node.is_dir,
            children: Vec::new(),
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tree line rendering
// ---------------------------------------------------------------------------

/// Recursively build styled `Line`s representing the file tree, emitting only
/// lines that fall within the visible viewport window (`visible_start..visible_end`).
///
/// `line_index` is a mutable counter tracking the current global line position
/// across the entire tree traversal.  Lines before `visible_start` increment
/// the counter without allocating `Line` objects; once past `visible_end` the
/// recursion short-circuits.
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
/// * `visible_start`  - First visible line index (inclusive).
/// * `visible_end`    - Last visible line index (exclusive).
/// * `line_index`     - Global line counter (mutated during traversal).
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
    visible_start: usize,
    visible_end: usize,
    line_index: &mut usize,
    lines: &mut Vec<Line<'static>>,
) {
    // If we have exceeded the maximum depth, emit a placeholder and return.
    if let Some(md) = max_depth {
        if current_depth > md {
            if *line_index >= visible_start && *line_index < visible_end {
                lines.push(Line::from(vec![Span::styled(
                    format!("{}...", prefix),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            *line_index += 1;
            return;
        }
    }

    let total = nodes.len();
    let display_count = match max_files {
        Some(mf) => mf.min(total),
        None => total,
    };

    for (i, node) in nodes.iter().enumerate() {
        // Early exit: all remaining lines are past the viewport.
        if *line_index >= visible_end {
            return;
        }

        if i >= display_count {
            if *line_index >= visible_start && *line_index < visible_end {
                let remaining = total - display_count;
                lines.push(Line::from(vec![Span::styled(
                    format!("{}... and {} more", prefix, remaining),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            *line_index += 1;
            break;
        }

        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        let visible = *line_index >= visible_start && *line_index < visible_end;

        if visible {
            // --- Build spans for this line ---

            let mut spans: Vec<Span<'static>> = Vec::new();

            // Track the visual (display) width consumed so far.
            let mut used_width: usize = 0;

            // 1. Prefix + connector
            let prefix_str = format!("{}{}", prefix, connector);
            used_width += UnicodeWidthStr::width(prefix_str.as_str());
            spans.push(Span::styled(
                prefix_str,
                Style::default().fg(Color::DarkGray),
            ));

            // 2. Emoji
            let emoji = get_file_emoji(&node.name, node.is_dir);
            let emoji_str = format!("{} ", emoji);
            used_width += UnicodeWidthStr::width(emoji_str.as_str());
            spans.push(Span::raw(emoji_str));

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

            used_width += UnicodeWidthStr::width(node.name.as_str());
            spans.push(Span::styled(node.name.clone(), name_style));

            // 4. Pad the name portion to fill the Name column (1 + NAME_WIDTH),
            //    then render the status badge in the Status column.
            let name_col_end = 1 + NAME_WIDTH as usize;
            let name_pad = if used_width < name_col_end {
                name_col_end - used_width
            } else {
                1 // at least one space if the name is very long
            };
            spans.push(Span::raw(" ".repeat(name_pad)));

            // 5. Status badge — rendered at the start of the Status column.
            if !status_text.is_empty() {
                let badge_color = match status_text {
                    "NEW" => Color::Green,
                    "MOD" => Color::Yellow,
                    "DEL" => Color::Red,
                    _ => Color::Reset,
                };
                let badge = format!("[{}]", status_text);
                let badge_width = badge.len();
                spans.push(Span::styled(
                    badge,
                    Style::default()
                        .fg(Color::Black)
                        .bg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ));
                // Pad the rest of the Status column after the badge.
                let status_pad = (STATUS_WIDTH as usize).saturating_sub(badge_width);
                spans.push(Span::raw(" ".repeat(status_pad)));
            } else {
                // No status — fill the entire Status column with spaces.
                spans.push(Span::raw(" ".repeat(STATUS_WIDTH as usize)));
            }

            // For files (not dirs), show size, delta, LOC, LOC delta as
            // fixed-width right-aligned columns.
            if !node.is_dir {
                if let Some(info) = state.get(&node.path) {
                    // 6. Size (right-aligned, SIZE_WIDTH)
                    let size_str = format_size(info.size);
                    let size_color = color_from_name(get_size_color(info.size));
                    spans.push(Span::styled(
                        format!("{:>width$}", size_str, width = SIZE_WIDTH as usize),
                        Style::default().fg(size_color),
                    ));

                    // 7. Size delta (right-aligned, DELTA_WIDTH)
                    let prev_size = previous_state.get(&node.path).map(|p| p.size).unwrap_or(0);
                    let size_delta = info.size as i64 - prev_size as i64;
                    let (delta_str, delta_color_name) = format_delta(size_delta, true);
                    spans.push(Span::styled(
                        format!("{:>width$}", delta_str, width = DELTA_WIDTH as usize),
                        Style::default().fg(color_from_name(delta_color_name)),
                    ));

                    // 8. LOC (right-aligned, LOC_WIDTH)
                    let loc_str = format_loc(info.loc);
                    spans.push(Span::styled(
                        format!("{:>width$}", loc_str, width = LOC_WIDTH as usize),
                        Style::default().fg(Color::DarkGray),
                    ));

                    // 9. LOC delta (right-aligned, LOC_WIDTH)
                    let prev_loc = previous_state.get(&node.path).map(|p| p.loc).unwrap_or(0);
                    let loc_delta = info.loc as i64 - prev_loc as i64;
                    let (loc_delta_str, loc_delta_color_name) = format_delta(loc_delta, false);
                    spans.push(Span::styled(
                        format!("{:>width$}", loc_delta_str, width = LOC_WIDTH as usize),
                        Style::default().fg(color_from_name(loc_delta_color_name)),
                    ));
                }
            }

            lines.push(Line::from(spans));
        }

        *line_index += 1;

        // Recurse into children for directories.
        if node.is_dir && !node.children.is_empty() {
            // If the entire subtree is before the viewport, skip it cheaply
            // by advancing the line counter without recursing into rendering.
            let subtree_size =
                count_tree_lines(&node.children, max_depth, max_files, current_depth + 1);
            if *line_index + subtree_size <= visible_start {
                *line_index += subtree_size;
            } else {
                render_tree_lines(
                    &node.children,
                    &child_prefix,
                    state,
                    changes,
                    previous_state,
                    max_depth,
                    max_files,
                    current_depth + 1,
                    visible_start,
                    visible_end,
                    line_index,
                    lines,
                );
            }
        }
    }
}

/// Count the total number of lines the tree would produce without building
/// any `Line` objects.  This mirrors the logic of `render_tree_lines` exactly
/// (including `max_depth` and `max_files` truncation) so the scroll indicator
/// stays accurate.
pub fn count_tree_lines(
    nodes: &[TreeNode],
    max_depth: Option<usize>,
    max_files: Option<usize>,
    current_depth: usize,
) -> usize {
    if let Some(md) = max_depth {
        if current_depth > md {
            return 1; // the "..." placeholder
        }
    }

    let total = nodes.len();
    let display_count = match max_files {
        Some(mf) => mf.min(total),
        None => total,
    };

    let mut count: usize = 0;

    for (i, node) in nodes.iter().enumerate() {
        if i >= display_count {
            count += 1; // "... and N more"
            break;
        }

        count += 1; // the node itself

        if node.is_dir && !node.children.is_empty() {
            count += count_tree_lines(&node.children, max_depth, max_files, current_depth + 1);
        }
    }

    count
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

/// Map activity buckets to a sparkline string using Unicode block characters.
///
/// Each bucket becomes one character whose height is proportional to the total
/// event count in that bucket relative to the maximum across all buckets.
/// Returns `(sparkline_string, colors)` where `colors` contains the dominant
/// colour for each bucket character.
fn build_sparkline(buckets: &[(usize, usize, usize)], width: usize) -> (String, Vec<Color>) {
    const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    // Resample buckets to `width` columns if the lengths differ.
    let resampled: Vec<(usize, usize, usize)> = if buckets.is_empty() {
        vec![(0, 0, 0); width]
    } else if buckets.len() == width {
        buckets.to_vec()
    } else {
        let mut out = vec![(0, 0, 0); width];
        for (i, b) in buckets.iter().enumerate() {
            let idx = (i * width) / buckets.len();
            let idx = idx.min(width - 1);
            out[idx].0 += b.0;
            out[idx].1 += b.1;
            out[idx].2 += b.2;
        }
        out
    };

    let max_total = resampled
        .iter()
        .map(|(c, m, d)| c + m + d)
        .max()
        .unwrap_or(0);

    let mut chars = String::with_capacity(width);
    let mut colors = Vec::with_capacity(width);

    for (c, m, d) in &resampled {
        let total = c + m + d;
        let level = if max_total == 0 {
            0
        } else {
            ((total * 8) / max_total).min(8)
        };
        chars.push(BLOCKS[level]);

        // Dominant colour: green for creates, yellow for modifies, red for deletes.
        let color = if total == 0 {
            Color::DarkGray
        } else if *c >= *m && *c >= *d {
            Color::Green
        } else if *m >= *c && *m >= *d {
            Color::Yellow
        } else {
            Color::Red
        };
        colors.push(color);
    }

    (chars, colors)
}

/// Render the development statistics dashboard.
fn render_stats_dashboard(frame: &mut Frame, area: Rect, stats: &Stats) {
    let duration_str = StatisticsTracker::format_duration(stats.session_duration);
    let events_rate = stats.events_per_minute;

    // --- Activity timeline sparkline ---
    let chart_width: usize = 50;
    let (sparkline, colors) = build_sparkline(&stats.activity_buckets, chart_width);

    let mut timeline_spans: Vec<Span<'static>> = vec![Span::styled(
        " Activity: ",
        Style::default().fg(Color::DarkGray),
    )];
    // Each character gets its own colour span.
    for (ch, color) in sparkline.chars().zip(colors.iter()) {
        timeline_spans.push(Span::styled(ch.to_string(), Style::default().fg(*color)));
    }

    // --- Top extensions line ---
    let mut ext_spans: Vec<Span<'static>> = vec![Span::styled(
        " Top types: ",
        Style::default().fg(Color::DarkGray),
    )];
    if stats.top_extensions.is_empty() {
        ext_spans.push(Span::styled("(none)", Style::default().fg(Color::DarkGray)));
    } else {
        for (i, (ext, count)) in stats.top_extensions.iter().enumerate() {
            if i > 0 {
                ext_spans.push(Span::styled(" ", Style::default().fg(Color::DarkGray)));
            }
            ext_spans.push(Span::styled(
                format!("{}({})", ext, count),
                Style::default().fg(Color::Cyan),
            ));
        }
    }

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
        Line::from(timeline_spans),
        Line::from(ext_spans),
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
/// Shows the search input bar when search is active, filter indicator when
/// a filter is applied, or the normal legend otherwise.  Includes a scroll
/// position indicator when the tree overflows the viewport.
#[allow(clippy::too_many_arguments)]
fn render_legend(
    frame: &mut Frame,
    area: Rect,
    search_query: &str,
    search_active: bool,
    scroll_offset: u16,
    total_lines: u16,
    viewport_height: u16,
    last_error: Option<&str>,
) {
    let mut spans: Vec<Span<'static>> = if search_active {
        // Search input mode: show the search bar with cursor.
        vec![
            Span::styled(
                " / ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}", search_query),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                "_",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::styled("  (Esc to cancel)", Style::default().fg(Color::DarkGray)),
        ]
    } else if !search_query.is_empty() {
        // Filter is active but not in input mode.
        vec![
            Span::styled(" Filter: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("\"{}\"", search_query),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  (Esc to clear, / to edit)  |  j/k scroll",
                Style::default().fg(Color::DarkGray),
            ),
        ]
    } else {
        // Normal legend.
        vec![
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
                "  |  q quit  / search  j/k scroll  g/G top/bottom",
                Style::default().fg(Color::DarkGray),
            ),
        ]
    };

    // Show watcher error if present.
    if let Some(err) = last_error {
        spans.push(Span::styled(
            format!("  [!] {}", err),
            Style::default().fg(Color::Red),
        ));
    }

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
    search_query: &str,
    search_active: bool,
    last_error: Option<&str>,
) -> u16 {
    let size = frame.area();

    // ----- Determine layout constraints -----

    let stats_height: u16 = if show_stats && stats.is_some() { 9 } else { 0 };

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
    let tree_nodes = if search_query.is_empty() {
        tree_nodes
    } else {
        filter_tree(&tree_nodes, search_query)
    };

    // Count total lines cheaply (no Line/Span allocations) for the scroll
    // indicator.  +1 for the column header row.
    let content_lines = count_tree_lines(&tree_nodes, max_depth, max_files, 0);
    let total_tree_lines = (content_lines + 1) as u16; // +1 for column headers

    // Virtual scrolling: only build Line objects for the visible viewport.
    let viewport_height = tree_area.height as usize;
    let scroll = scroll_offset as usize;

    let mut tree_lines: Vec<Line<'static>> = Vec::with_capacity(viewport_height);

    // The column header is always line 0.
    if scroll == 0 {
        tree_lines.push(tree_column_headers());
    }

    // Content lines start at global index 1 (after the header).
    // Determine the visible window within content lines.
    let content_visible_start = if scroll == 0 {
        0
    } else {
        scroll.saturating_sub(1)
    };
    let remaining_viewport = viewport_height.saturating_sub(tree_lines.len());
    let content_visible_end = content_visible_start + remaining_viewport;

    let mut line_index: usize = 0;
    render_tree_lines(
        &tree_nodes,
        " ",
        state,
        changes,
        previous_state,
        max_depth,
        max_files,
        0,
        content_visible_start,
        content_visible_end,
        &mut line_index,
        &mut tree_lines,
    );

    let tree_text = Text::from(tree_lines);
    let tree_block = Block::default().borders(Borders::NONE);
    let tree_paragraph = Paragraph::new(tree_text).block(tree_block);
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
        search_query,
        search_active,
        scroll_offset,
        total_tree_lines,
        tree_area.height,
        last_error,
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
                size: 100,
                modified: 0.0,
                is_dir: false,
                loc: 10,
            },
        );
        state.insert(
            PathBuf::from("/project/alpha"),
            FileInfo {
                size: 0,
                modified: 0.0,
                is_dir: true,
                loc: 0,
            },
        );
        state.insert(
            PathBuf::from("/project/beta.rs"),
            FileInfo {
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
        let mut line_index = 0;

        render_tree_lines(
            &nodes,
            " ",
            &state,
            &changes,
            &previous_state,
            None,
            None,
            0,
            0,
            usize::MAX,
            &mut line_index,
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
                size: 0,
                modified: 0.0,
                is_dir: true,
                loc: 0,
            },
        );
        state.insert(
            PathBuf::from("/project/src/main.rs"),
            FileInfo {
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
        let mut line_index = 0;

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
            0,
            usize::MAX,
            &mut line_index,
            &mut lines,
        );

        // Should have the "src" dir line, but its children should be replaced
        // by a "..." placeholder.
        assert!(lines.len() >= 1);
    }
}
