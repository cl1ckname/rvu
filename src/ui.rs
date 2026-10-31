//! Rendering. Reads state, never changes it.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

use crate::highlight::{HighlightCache, expand_tabs};
use crate::keymap::{DEFAULT_BINDINGS, HELP_SECTIONS, keys_for};
use crate::model::{Changeset, FileStatus, LineKind, Side};
use crate::tree::{FileTree, ItemKind};
use crate::view::{DiffView, Row};

const ADDED_BG: Color = Color::Rgb(0x1f, 0x36, 0x27);
const REMOVED_BG: Color = Color::Rgb(0x42, 0x1f, 0x25);
const CURSOR_BG: Color = Color::Rgb(0x2e, 0x33, 0x40);
const CURSOR_ADDED_BG: Color = Color::Rgb(0x2c, 0x52, 0x3a);
const CURSOR_REMOVED_BG: Color = Color::Rgb(0x60, 0x2b, 0x34);
const HEADER_BG: Color = Color::Rgb(0x26, 0x2a, 0x33);
const GUTTER_FG: Color = Color::Rgb(0x5c, 0x63, 0x70);
const ADDED_FG: Color = Color::Rgb(0x8f, 0xd1, 0x9e);
const REMOVED_FG: Color = Color::Rgb(0xf0, 0x8a, 0x95);
const FOCUS_BORDER: Color = Color::Rgb(0x81, 0xa1, 0xc1);

pub fn pane_block(title: Line<'_>, focused: bool) -> Block<'_> {
    let block = Block::bordered().border_type(BorderType::Rounded).title(title);
    if focused {
        block.border_style(Style::default().fg(FOCUS_BORDER))
    } else {
        block.border_style(Style::default().fg(GUTTER_FG))
    }
}

pub fn status_color(status: FileStatus) -> Color {
    match status {
        FileStatus::Added => ADDED_FG,
        FileStatus::Deleted => REMOVED_FG,
        FileStatus::Modified | FileStatus::Renamed => Color::Rgb(0xeb, 0xcb, 0x8b),
        FileStatus::Other => GUTTER_FG,
    }
}

pub fn draw_tree(buf: &mut Buffer, area: Rect, tree: &FileTree, cs: &Changeset, focused: bool) {
    for (row, (item, collapsed)) in tree
        .visible_items()
        .enumerate()
        .skip(tree.offset)
        .take(area.height as usize)
    {
        let y = area.y + (row - tree.offset) as u16;
        let indent = "  ".repeat(item.depth);
        let line = match item.kind {
            ItemKind::Dir { .. } => {
                let icon = if collapsed { "▸ " } else { "▾ " };
                Line::from(vec![indent.into(), icon.dim(), Span::from(item.label.as_str()).bold()])
            }
            ItemKind::File { file } => {
                let status = cs.files[file].status;
                Line::from(vec![
                    indent.into(),
                    Span::styled(format!("{} ", status.letter()), Style::default().fg(status_color(status))),
                    item.label.as_str().into(),
                ])
            }
        };
        let row_area = Rect { y, height: 1, ..area };
        if row == tree.selected {
            let bg = if focused { CURSOR_BG } else { HEADER_BG };
            buf.set_style(row_area, Style::default().bg(bg));
        }
        buf.set_line(area.x, y, &line, area.width);
    }
}

/// Width of a line number column: enough for the largest number, at least 3.
pub fn gutter_digits(cs: &Changeset) -> usize {
    let max = cs
        .files
        .iter()
        .flat_map(|f| &f.hunks)
        .flat_map(|h| &h.lines)
        .flat_map(|l| [l.old_lineno, l.new_lineno])
        .flatten()
        .max()
        .unwrap_or(0);
    max.to_string().len().max(3)
}

pub fn draw_diff(
    buf: &mut Buffer,
    area: Rect,
    view: &DiffView,
    cs: &Changeset,
    hl: &HighlightCache,
    digits: usize,
) {
    if view.rows.is_empty() {
        let msg = Paragraph::new("No changes").alignment(Alignment::Center).dim();
        msg.render(Rect { y: area.y + area.height / 2, height: 1, ..area }, buf);
        return;
    }
    for (i, idx) in view.visible().enumerate() {
        let y = area.y + i as u16;
        let row_area = Rect { y, height: 1, ..area };
        let is_cursor = idx == view.cursor;
        let (line, bg) = render_row(view.rows[idx], cs, hl, digits, is_cursor);
        if let Some(bg) = bg {
            buf.set_style(row_area, Style::default().bg(bg));
        }
        buf.set_line(area.x, y, &line, area.width);
    }
}

fn render_row(
    row: Row,
    cs: &Changeset,
    hl: &HighlightCache,
    digits: usize,
    is_cursor: bool,
) -> (Line<'static>, Option<Color>) {
    let gutter = Style::default().fg(GUTTER_FG);
    let cursor_bg = is_cursor.then_some(CURSOR_BG);
    match row {
        Row::Spacer => (Line::default(), cursor_bg),
        Row::FileHeader { file } => {
            let f = &cs.files[file];
            let mut spans = vec![
                Span::styled(format!(" {} ", f.status.letter()), Style::default().fg(status_color(f.status)).bold()),
                Span::from(f.path().display().to_string()).bold(),
            ];
            if f.status == FileStatus::Renamed
                && let Some(old) = &f.old_path
            {
                spans.push(Span::styled(format!("  ← {}", old.display()), gutter));
            }
            let (added, removed) = file_stats(cs, file);
            spans.push(Span::styled(format!("  +{added}"), Style::default().fg(ADDED_FG)));
            spans.push(Span::styled(format!(" -{removed}"), Style::default().fg(REMOVED_FG)));
            (Line::from(spans), Some(if is_cursor { CURSOR_BG } else { HEADER_BG }))
        }
        Row::HunkHeader { file, hunk } => {
            let header = &cs.files[file].hunks[hunk].header;
            let pad = " ".repeat(digits * 2 + 3);
            let line = Line::from(vec![
                Span::from(pad),
                Span::styled(header.clone(), Style::default().fg(Color::Rgb(0x88, 0xc0, 0xd0)).add_modifier(Modifier::DIM)),
            ]);
            (line, cursor_bg)
        }
        Row::Note { text, .. } => {
            let pad = " ".repeat(digits * 2 + 3);
            (Line::from(vec![Span::from(pad), Span::from(text).italic().dim()]), cursor_bg)
        }
        Row::Code { file, hunk, line } => {
            let l = &cs.files[file].hunks[hunk].lines[line];
            let n = (l.old_lineno, l.new_lineno);
            code_line(file, l.kind, n, &l.content, hl, digits, is_cursor)
        }
        Row::Fold { hidden, .. } => {
            let pad = " ".repeat(digits * 2 + 3);
            let text = format!("⋯ {hidden} unchanged lines");
            let style = Style::default().fg(GUTTER_FG).italic();
            (Line::from(vec![Span::from(pad), Span::styled(text, style)]), Some(if is_cursor { CURSOR_BG } else { HEADER_BG }))
        }
        Row::FileLine { file, old, new } => {
            let text = cs.files[file].new_content.as_ref().and_then(|t| t.line(new)).unwrap_or("");
            code_line(file, LineKind::Context, (Some(old), Some(new)), text, hl, digits, is_cursor)
        }
    }
}

fn code_line(
    file: usize,
    kind: LineKind,
    (old, new): (Option<u32>, Option<u32>),
    content: &str,
    hl: &HighlightCache,
    digits: usize,
    is_cursor: bool,
) -> (Line<'static>, Option<Color>) {
    let num = |n: Option<u32>| n.map(|n| format!("{n:>digits$}")).unwrap_or_else(|| " ".repeat(digits));
    let (sign_fg, bg) = match (kind, is_cursor) {
        (LineKind::Context, false) => (GUTTER_FG, None),
        (LineKind::Context, true) => (GUTTER_FG, Some(CURSOR_BG)),
        (LineKind::Added, false) => (ADDED_FG, Some(ADDED_BG)),
        (LineKind::Added, true) => (ADDED_FG, Some(CURSOR_ADDED_BG)),
        (LineKind::Removed, false) => (REMOVED_FG, Some(REMOVED_BG)),
        (LineKind::Removed, true) => (REMOVED_FG, Some(CURSOR_REMOVED_BG)),
    };
    let gutter = Style::default().fg(GUTTER_FG);
    let gutter_style = if is_cursor { gutter.fg(Color::White).bold() } else { gutter };
    let mut spans = vec![
        Span::styled(format!("{} {} ", num(old), num(new)), gutter_style),
        Span::styled(format!("{} ", kind.sign()), Style::default().fg(sign_fg)),
    ];
    let text = expand_tabs(content);
    let side = kind.side();
    let lineno = match side {
        Side::Old => old,
        Side::New => new,
    };
    match lineno.and_then(|n| hl.line(file, side, n, &text)) {
        Some(styled) => spans.extend(styled.iter().cloned()),
        None => spans.push(Span::from(text)),
    }
    (Line::from(spans), bg)
}

pub fn file_stats(cs: &Changeset, file: usize) -> (usize, usize) {
    let lines = cs.files[file].hunks.iter().flat_map(|h| &h.lines);
    lines.fold((0, 0), |(a, r), l| match l.kind {
        LineKind::Added => (a + 1, r),
        LineKind::Removed => (a, r + 1),
        LineKind::Context => (a, r),
    })
}

/// Cheatsheet popup. Returns the scroll position clamped to the content.
pub fn draw_help(buf: &mut Buffer, area: Rect, scroll: usize) -> usize {
    // Sections as lists of (keys, description); the title has empty keys.
    let sections: Vec<Vec<(String, &str)>> = HELP_SECTIONS
        .iter()
        .map(|(title, actions)| {
            let mut lines = vec![(String::new(), *title)];
            lines.extend(actions.iter().map(|a| (keys_for(DEFAULT_BINDINGS, *a), a.description())));
            lines
        })
        .collect();
    let all = || sections.iter().flatten();
    let key_w = all().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    let desc_w = all().map(|(_, d)| d.chars().count()).max().unwrap_or(0);
    let col_w = key_w + 2 + desc_w;
    const GAP: usize = 4;

    let column = |secs: &[Vec<(String, &str)>]| -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for (i, sec) in secs.iter().enumerate() {
            if i > 0 {
                out.push(Line::default());
            }
            for (keys, desc) in sec {
                if keys.is_empty() {
                    out.push(Line::from(Span::from(format!("{desc:<col_w$}")).bold().underlined()));
                } else {
                    out.push(Line::from(vec![
                        Span::styled(format!("{keys:<key_w$}  "), Style::default().fg(FOCUS_BORDER).bold()),
                        Span::from(format!("{desc:<desc_w$}")),
                    ]));
                }
            }
        }
        out
    };

    // Two columns if they fit: split sections so that columns are balanced.
    let two_columns = area.width as usize >= 2 * col_w + GAP + 4;
    let body: Vec<Line> = if two_columns {
        let total: usize = sections.iter().map(|s| s.len() + 1).sum();
        let mut split = 0;
        let mut left_len = 0;
        while split < sections.len() && left_len + sections[split].len() < total / 2 + 1 {
            left_len += sections[split].len() + 1;
            split += 1;
        }
        let (left, right) = (column(&sections[..split.max(1)]), column(&sections[split.max(1)..]));
        (0..left.len().max(right.len()))
            .map(|i| {
                let mut spans = left.get(i).cloned().unwrap_or_else(|| Line::from(" ".repeat(col_w))).spans;
                if spans.is_empty() {
                    spans.push(Span::from(" ".repeat(col_w)));
                }
                spans.push(Span::from(" ".repeat(GAP)));
                spans.extend(right.get(i).cloned().unwrap_or_default().spans);
                Line::from(spans)
            })
            .collect()
    } else {
        column(&sections)
    };

    let mut lines = vec![
        Line::from("Prefix motions with a count: 5j, 3]c".italic().dim()),
        Line::default(),
    ];
    lines.extend(body);

    let content_w = if two_columns { 2 * col_w + GAP } else { col_w };
    let width = (content_w as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let visible = height.saturating_sub(2) as usize;
    let scroll = scroll.min(lines.len().saturating_sub(visible));
    let footer = if lines.len() > visible { " j/k scroll · ? q Esc close " } else { " ? q Esc close " };

    Clear.render(popup, buf);
    Paragraph::new(lines)
        .scroll((scroll as u16, 0))
        .block(
            pane_block(Line::from(" Cheatsheet ").bold(), true)
                .title_bottom(Line::from(footer).dim().right_aligned())
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .render(popup, buf);
    scroll
}
