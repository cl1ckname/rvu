//! Diff view state: flat list of rows, cursor and scroll position.
//! Rendering lives in `ui`; this module is pure logic.

use crate::model::{Changeset, LineKind};

/// Lines kept visible above/below the cursor when scrolling.
const SCROLLOFF: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// Empty line between files.
    Spacer,
    FileHeader { file: usize },
    HunkHeader { file: usize, hunk: usize },
    Code { file: usize, hunk: usize, line: usize },
    /// Unchanged line outside hunks (Page mode). Exists in both versions.
    FileLine { file: usize, old: u32, new: u32 },
    /// `hidden` unchanged lines folded away (Page mode); `old`/`new` are
    /// the numbers of the first hidden line.
    Fold { file: usize, old: u32, new: u32, hidden: u32 },
    /// "Binary file", "No textual changes".
    Note { file: usize, text: &'static str },
}

impl Row {
    pub fn file(self) -> Option<usize> {
        match self {
            Row::Spacer => None,
            Row::FileHeader { file }
            | Row::HunkHeader { file, .. }
            | Row::Code { file, .. }
            | Row::FileLine { file, .. }
            | Row::Fold { file, .. }
            | Row::Note { file, .. } => Some(file),
        }
    }

    /// `(old, new)` line numbers of a code row.
    pub fn line_numbers(self, cs: &Changeset) -> Option<(Option<u32>, Option<u32>)> {
        match self {
            Row::Code { file, hunk, line } => {
                let l = &cs.files[file].hunks[hunk].lines[line];
                Some((l.old_lineno, l.new_lineno))
            }
            Row::FileLine { old, new, .. } | Row::Fold { old, new, .. } => Some((Some(old), Some(new))),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct DiffView {
    pub rows: Vec<Row>,
    pub cursor: usize,
    /// Index of the first visible row.
    pub offset: usize,
    /// Number of visible rows, updated on each draw.
    pub height: usize,
}

impl DiffView {
    /// Scroll mode: all files one after another, like `git diff`.
    pub fn scroll(cs: &Changeset) -> Self {
        let mut rows = Vec::new();
        for (fi, f) in cs.files.iter().enumerate() {
            if fi > 0 {
                rows.push(Row::Spacer);
            }
            rows.push(Row::FileHeader { file: fi });
            if f.binary {
                rows.push(Row::Note { file: fi, text: "Binary file" });
            } else if f.too_large {
                rows.push(Row::Note { file: fi, text: "Diff is too large to display" });
            } else if f.hunks.is_empty() {
                rows.push(Row::Note { file: fi, text: "No textual changes" });
            }
            for (hi, h) in f.hunks.iter().enumerate() {
                rows.push(Row::HunkHeader { file: fi, hunk: hi });
                for li in 0..h.lines.len() {
                    rows.push(Row::Code { file: fi, hunk: hi, line: li });
                }
            }
        }
        Self { rows, ..Self::default() }
    }

    /// Page mode: one whole file (new version) with hunks inlined:
    /// removed lines are shown at the place where they used to be.
    /// With `fold = Some(n)`, unchanged lines farther than `n` lines from
    /// any change are folded.
    pub fn page(cs: &Changeset, fi: usize, fold: Option<usize>) -> Self {
        let mut view = Self::page_unfolded(cs, fi);
        if let Some(context) = fold {
            view.rows = fold_unchanged(cs, view.rows, context);
        }
        view
    }

    fn page_unfolded(cs: &Changeset, fi: usize) -> Self {
        let f = &cs.files[fi];
        let mut rows = vec![Row::FileHeader { file: fi }];
        if f.binary {
            rows.push(Row::Note { file: fi, text: "Binary file" });
        } else if f.too_large {
            rows.push(Row::Note { file: fi, text: "Diff is too large to display" });
        } else if f.hunks.is_empty() {
            rows.push(Row::Note { file: fi, text: "No textual changes" });
        }
        let push_hunk = |rows: &mut Vec<Row>, hi: usize| {
            rows.extend((0..f.hunks[hi].lines.len()).map(|li| Row::Code { file: fi, hunk: hi, line: li }));
        };

        let Some(text) = f.new_content.as_ref() else {
            // Deleted file or new version unavailable: hunks are all we have.
            (0..f.hunks.len()).for_each(|hi| push_hunk(&mut rows, hi));
            return Self { rows, ..Self::default() };
        };

        // Next new-version line to emit, and (old - new) shift for unchanged lines.
        let mut next_new = 1u32;
        let mut shift = 0i64;
        let push_unchanged = |rows: &mut Vec<Row>, from: u32, upto: u32, shift: i64| {
            for new in from..=upto {
                rows.push(Row::FileLine { file: fi, old: (new as i64 + shift) as u32, new });
            }
        };
        for (hi, h) in f.hunks.iter().enumerate() {
            // For empty ranges git reports the line *before* the change.
            let new_before = if h.new_len == 0 { h.new_start } else { h.new_start.saturating_sub(1) };
            let old_before = if h.old_len == 0 { h.old_start } else { h.old_start.saturating_sub(1) };
            push_unchanged(&mut rows, next_new, new_before, shift);
            push_hunk(&mut rows, hi);
            next_new = new_before + h.new_len + 1;
            shift = (old_before + h.old_len) as i64 - (new_before + h.new_len) as i64;
        }
        push_unchanged(&mut rows, next_new, text.line_count(), shift);
        Self { rows, ..Self::default() }
    }

    /// Row showing the given line of `file`: exact match if present
    /// (by new line number, or old for removed lines), otherwise the nearest
    /// line of the file, otherwise the file header.
    pub fn find_line(&self, cs: &Changeset, file: usize, old: Option<u32>, new: Option<u32>) -> Option<usize> {
        let key = |r: Row| -> Option<u32> {
            let (o, n) = r.line_numbers(cs)?;
            if new.is_some() { n } else { o }
        };
        let target = new.or(old);
        let distance = |r: Row, k: u32, t: u32| match r {
            // A line hidden in a fold is "at" the fold.
            Row::Fold { hidden, .. } if (k..k + hidden).contains(&t) => 0,
            _ => k.abs_diff(t),
        };
        let candidates = self.rows.iter().enumerate().filter(|(_, r)| r.file() == Some(file));
        let nearest = target.and_then(|t| {
            candidates
                .filter_map(|(i, r)| key(*r).map(|k| (i, distance(*r, k, t))))
                .min_by_key(|&(_, d)| d)
                .map(|(i, _)| i)
        });
        nearest.or_else(|| self.file_row(file))
    }

    pub fn current(&self) -> Option<Row> {
        self.rows.get(self.cursor).copied()
    }

    pub fn current_file(&self) -> Option<usize> {
        self.current().and_then(Row::file)
    }

    pub fn visible(&self) -> std::ops::Range<usize> {
        self.offset..(self.offset + self.height).min(self.rows.len())
    }

    fn last(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }

    /// Moves the cursor and scrolls minimally to keep it visible.
    pub fn goto(&mut self, row: usize) {
        self.cursor = row.min(self.last());
        self.ensure_visible();
    }

    pub fn move_by(&mut self, delta: isize) {
        self.goto(self.cursor.saturating_add_signed(delta));
    }

    /// `Ctrl-d` / `Ctrl-u` / `Ctrl-f` / `Ctrl-b`: move both view and cursor.
    /// Like nvim, the view moves first and the cursor is then pushed inside the
    /// scrolloff margin instead of the view being pulled back to the cursor.
    pub fn scroll_by(&mut self, delta: isize) {
        let max_offset = self.rows.len().saturating_sub(self.height);
        self.offset = self.offset.saturating_add_signed(delta).min(max_offset);
        let mut cursor = self.cursor.saturating_add_signed(delta).min(self.last());
        let margin = self.margin();
        if self.offset > 0 {
            cursor = cursor.max(self.offset + margin);
        }
        if self.offset < max_offset {
            cursor = cursor.min((self.offset + self.height).saturating_sub(margin + 1));
        }
        self.goto(cursor);
    }

    fn margin(&self) -> usize {
        SCROLLOFF.min(self.height.saturating_sub(1) / 2)
    }

    pub fn cursor_to_center(&mut self) {
        self.offset = self.cursor.saturating_sub(self.height / 2);
    }

    pub fn cursor_to_top(&mut self) {
        self.offset = self.cursor;
    }

    pub fn cursor_to_bottom(&mut self) {
        self.offset = (self.cursor + 1).saturating_sub(self.height);
    }

    /// Keeps the cursor inside the viewport with `SCROLLOFF` margin.
    pub fn ensure_visible(&mut self) {
        if self.height == 0 {
            return;
        }
        let margin = self.margin();
        if self.cursor < self.offset + margin {
            self.offset = self.cursor.saturating_sub(margin);
        } else if self.cursor + margin >= self.offset + self.height {
            self.offset = self.cursor + margin + 1 - self.height;
        }
        let max_offset = self.rows.len().saturating_sub(self.height);
        self.offset = self.offset.min(max_offset);
    }

    /// Row where a block of consecutive added/removed lines starts.
    fn is_change_start(&self, cs: &Changeset, i: usize) -> bool {
        let is_change = |i: usize| match self.rows.get(i) {
            Some(&Row::Code { file, hunk, line }) => {
                cs.files[file].hunks[hunk].lines[line].kind.is_change()
            }
            _ => false,
        };
        is_change(i) && (i == 0 || !is_change(i - 1))
    }

    /// `]c` / `[c`: jumps `count` change blocks forward or backward.
    /// Returns how many jumps were made (less than `count` at the edges).
    pub fn jump_change(&mut self, cs: &Changeset, forward: bool, count: usize) -> usize {
        self.jump_to(count, forward, |v, i| v.is_change_start(cs, i))
    }

    /// `]f` / `[f`.
    pub fn jump_file(&mut self, forward: bool, count: usize) {
        self.jump_to(count, forward, |v, i| matches!(v.rows[i], Row::FileHeader { .. }));
    }

    /// Text of a code row; `None` for headers and notes.
    fn row_text<'a>(&self, cs: &'a Changeset, i: usize) -> Option<&'a str> {
        match self.rows[i] {
            Row::Spacer => Some(""),
            Row::Code { file, hunk, line } => Some(&cs.files[file].hunks[hunk].lines[line].content),
            Row::FileLine { file, new, .. } => cs.files[file].new_content.as_ref()?.line(new),
            Row::FileHeader { .. } | Row::HunkHeader { .. } | Row::Note { .. } | Row::Fold { .. } => None,
        }
    }

    /// Paragraph boundary for `{` / `}`: an empty or whitespace-only line.
    fn is_blank(&self, cs: &Changeset, i: usize) -> bool {
        self.row_text(cs, i).is_some_and(|t| t.trim().is_empty())
    }

    /// `}` / `{` like in vim: skip blank lines, then non-blank ones, stop at
    /// the next blank line (or at the first/last row).
    pub fn jump_paragraph(&mut self, cs: &Changeset, forward: bool, count: usize) {
        let mut pos = self.cursor;
        for _ in 0..count {
            if forward {
                let mut i = pos + 1;
                while i < self.rows.len() && self.is_blank(cs, i) {
                    i += 1;
                }
                while i < self.rows.len() && !self.is_blank(cs, i) {
                    i += 1;
                }
                pos = i.min(self.last());
            } else {
                let mut i = pos;
                while i > 0 && self.is_blank(cs, i - 1) {
                    i -= 1;
                }
                while i > 0 && !self.is_blank(cs, i - 1) {
                    i -= 1;
                }
                pos = i.saturating_sub(1);
            }
        }
        self.goto(pos);
    }

    /// First / last change block, if any.
    pub fn edge_change(&self, cs: &Changeset, first: bool) -> Option<usize> {
        let mut it = (0..self.rows.len()).filter(|&i| self.is_change_start(cs, i));
        if first { it.next() } else { it.next_back() }
    }

    fn jump_to(&mut self, count: usize, forward: bool, pred: impl Fn(&Self, usize) -> bool) -> usize {
        let mut done = 0;
        let mut target = None;
        let mut pos = self.cursor;
        for _ in 0..count {
            let next = if forward {
                (pos + 1..self.rows.len()).find(|&i| pred(self, i))
            } else {
                (0..pos).rev().find(|&i| pred(self, i))
            };
            match next {
                Some(i) => {
                    target = Some(i);
                    pos = i;
                    done += 1;
                }
                None => break,
            }
        }
        if let Some(t) = target {
            self.goto(t);
        }
        done
    }

    pub fn file_row(&self, file: usize) -> Option<usize> {
        self.rows.iter().position(|r| *r == Row::FileHeader { file })
    }

    /// Line kind under the cursor, if it is a code line.
    pub fn current_kind(&self, cs: &Changeset) -> Option<LineKind> {
        match self.current()? {
            Row::Code { file, hunk, line } => Some(cs.files[file].hunks[hunk].lines[line].kind),
            _ => None,
        }
    }
}

/// Replaces runs of unchanged lines farther than `context` rows from any
/// change with a single `Row::Fold`. Runs of one line are left as is:
/// the fold marker would take the same space.
fn fold_unchanged(cs: &Changeset, rows: Vec<Row>, context: usize) -> Vec<Row> {
    let is_change = |r: &Row| match *r {
        Row::Code { file, hunk, line } => cs.files[file].hunks[hunk].lines[line].kind.is_change(),
        _ => false,
    };
    let is_unchanged = |r: &Row| match *r {
        Row::FileLine { .. } => true,
        Row::Code { .. } => !is_change(r),
        _ => false,
    };

    // Distance (in rows) to the nearest change: one pass in each direction.
    let n = rows.len();
    let mut dist = vec![usize::MAX; n];
    let mut last = None;
    for i in 0..n {
        if is_change(&rows[i]) {
            last = Some(i);
        }
        if let Some(l) = last {
            dist[i] = i - l;
        }
    }
    last = None;
    for i in (0..n).rev() {
        if is_change(&rows[i]) {
            last = Some(i);
        }
        if let Some(l) = last {
            dist[i] = dist[i].min(l - i);
        }
    }

    let hide = |i: usize| is_unchanged(&rows[i]) && dist[i] > context;
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if !hide(i) {
            out.push(rows[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < n && hide(i) {
            i += 1;
        }
        let file = rows[start].file().expect("code row without file");
        let (old, new) = rows[start].line_numbers(cs).expect("unchanged row without numbers");
        match (i - start, old, new) {
            (1, _, _) => out.push(rows[start]),
            (hidden, Some(old), Some(new)) => {
                out.push(Row::Fold { file, old, new, hidden: hidden as u32 })
            }
            _ => out.extend_from_slice(&rows[start..i]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DiffLine, FileDiff, FileStatus, Hunk};

    fn line(kind: LineKind) -> DiffLine {
        DiffLine { kind, old_lineno: Some(1), new_lineno: Some(1), content: String::new() }
    }

    fn file(kinds: &[LineKind]) -> FileDiff {
        FileDiff {
            old_path: Some("a".into()),
            new_path: Some("a".into()),
            status: FileStatus::Modified,
            binary: false,
            too_large: false,
            hunks: vec![Hunk {
                header: "@@".into(),
                old_start: 1,
                old_len: 0,
                new_start: 1,
                new_len: 0,
                lines: kinds.iter().map(|k| line(*k)).collect(),
            }],
            old_content: None,
            new_content: None,
        }
    }

    use LineKind::{Added as A, Context as C, Removed as R};

    fn cs() -> Changeset {
        // rows: 0 header, 1 hunk, 2 C, 3 R, 4 A, 5 C, 6 A, 7 spacer, 8 header, 9 hunk, 10 A
        Changeset { files: vec![file(&[C, R, A, C, A]), file(&[A])] }
    }

    #[test]
    fn rows_layout() {
        let v = DiffView::scroll(&cs());
        assert_eq!(v.rows.len(), 11);
        assert_eq!(v.rows[7], Row::Spacer);
        assert_eq!(v.rows[8], Row::FileHeader { file: 1 });
    }

    #[test]
    fn jump_between_changes_and_files() {
        let cs = cs();
        let mut v = DiffView::scroll(&cs);
        v.height = 5;
        v.jump_change(&cs, true, 1);
        assert_eq!(v.cursor, 3); // R+A is one block
        v.jump_change(&cs, true, 1);
        assert_eq!(v.cursor, 6);
        v.jump_change(&cs, true, 1);
        assert_eq!(v.cursor, 10); // crosses file boundary
        v.jump_change(&cs, true, 1);
        assert_eq!(v.cursor, 10); // nothing further: stay
        v.jump_change(&cs, false, 2);
        assert_eq!(v.cursor, 3);
        v.jump_file(true, 1);
        assert_eq!(v.cursor, 8);
        v.jump_file(false, 5);
        assert_eq!(v.cursor, 0);
    }

    #[test]
    fn scrolling_keeps_cursor_visible() {
        let mut v = DiffView::scroll(&cs());
        v.height = 4;
        v.goto(10);
        assert!(v.visible().contains(&10));
        assert_eq!(v.offset, 7); // clamped to the end
        v.goto(0);
        assert_eq!(v.offset, 0);
        v.move_by(-5);
        assert_eq!(v.cursor, 0);
        v.scroll_by(2);
        assert_eq!((v.offset, v.cursor), (2, 3));
    }

    mod page {
        use super::*;
        use crate::source::local::LocalRepo;

        /// Real diff of `old` -> `new` with the given context.
        fn diff(old: &str, new: &str, context: u32) -> (tempfile::TempDir, Changeset) {
            let dir = tempfile::tempdir().unwrap();
            let repo = git2::Repository::init(dir.path()).unwrap();
            std::fs::write(dir.path().join("f.txt"), old).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("f.txt")).unwrap();
            index.write().unwrap();
            std::fs::write(dir.path().join("f.txt"), new).unwrap();
            let cs = LocalRepo::discover(dir.path()).unwrap().working_tree_diff(context).unwrap();
            (dir, cs)
        }

        /// Renders rows as "<sign><old>/<new>" for compact assertions.
        fn render(cs: &Changeset, v: &DiffView) -> Vec<String> {
            v.rows
                .iter()
                .filter_map(|r| {
                    let (o, n) = r.line_numbers(cs)?;
                    let sign = match r {
                        Row::Code { file, hunk, line } => cs.files[*file].hunks[*hunk].lines[*line].kind.sign(),
                        _ => '=',
                    };
                    let f = |x: Option<u32>| x.map(|x| x.to_string()).unwrap_or_default();
                    Some(format!("{sign}{}/{}", f(o), f(n)))
                })
                .collect()
        }

        const OLD: &str = "1\n2\n3\n4\n5\n6\n7\n8\n";

        #[test]
        fn whole_file_with_inlined_hunks() {
            // remove 2, change 5, add after 7
            let (_d, cs) = diff(OLD, "1\n3\n4\nfive\n6\n7\nnew\n8\n", 0);
            let v = DiffView::page(&cs, 0, None);
            assert_eq!(
                render(&cs, &v),
                ["=1/1", "-2/", "=3/2", "=4/3", "-5/", "+/4", "=6/5", "=7/6", "+/7", "=8/8"]
            );
        }

        #[test]
        fn removal_at_start_and_end() {
            let (_d, cs) = diff(OLD, "2\n3\n4\n5\n6\n7\n", 0);
            let v = DiffView::page(&cs, 0, None);
            assert_eq!(
                render(&cs, &v),
                ["-1/", "=2/1", "=3/2", "=4/3", "=5/4", "=6/5", "=7/6", "-8/"]
            );
        }

        #[test]
        fn with_context_lines() {
            let (_d, cs) = diff(OLD, "1\n2\n3\n4\nfive\n6\n7\n8\n", 1);
            let v = DiffView::page(&cs, 0, None);
            assert_eq!(
                render(&cs, &v),
                ["=1/1", "=2/2", "=3/3", " 4/4", "-5/", "+/5", " 6/6", "=7/7", "=8/8"]
            );
        }

        #[test]
        fn folding() {
            let old: String = (1..=30).map(|i| format!("{i}\n")).collect();
            let new = old.replace("\n10\n", "\nten\n").replace("\n12\n", "\ntwelve\n");
            let (_d, cs) = diff(&old, &new, 0);
            let v = DiffView::page(&cs, 0, Some(2));
            let folds: Vec<_> = v
                .rows
                .iter()
                .filter_map(|r| match *r {
                    Row::Fold { new, hidden, .. } => Some((new, hidden)),
                    _ => None,
                })
                .collect();
            // 1..=7 hidden, 8 9 [10] 11 [12] 13 14 shown, 15..=30 hidden.
            assert_eq!(folds, [(1, 7), (15, 16)]);

            // A hidden line is found at its fold.
            let row = v.find_line(&cs, 0, Some(20), Some(20)).unwrap();
            assert!(matches!(v.rows[row], Row::Fold { new: 15, .. }));

            // Large context: nothing to fold. Single-line runs are not folded.
            assert!(!DiffView::page(&cs, 0, Some(100)).rows.iter().any(|r| matches!(r, Row::Fold { .. })));
            let v = DiffView::page(&cs, 0, Some(8));
            assert!(v.rows.iter().any(|r| matches!(r, Row::Fold { new: 21, hidden: 10, .. })));
            assert!(!v.rows.iter().any(|r| matches!(r, Row::Fold { new: 1, .. })));
        }

        #[test]
        fn paragraphs() {
            // Blank lines at new 3 and 6 (the second one is whitespace-only).
            let text = "a\nb\n\nc\nd\n  \ne\n";
            let (_d, cs) = diff(text, text.replace('e', "E").as_str(), 0);
            let mut v = DiffView::page(&cs, 0, None);
            v.height = 50;
            let new_of = |v: &DiffView| v.current().and_then(|r| r.line_numbers(&cs)).and_then(|(_, n)| n);
            v.goto(1); // line a
            v.jump_paragraph(&cs, true, 1);
            assert_eq!(new_of(&v), Some(3));
            v.jump_paragraph(&cs, true, 1);
            assert_eq!(new_of(&v), Some(6));
            v.jump_paragraph(&cs, true, 1); // no more blanks: last row
            assert_eq!(v.cursor, v.rows.len() - 1);
            v.jump_paragraph(&cs, false, 2);
            assert_eq!(new_of(&v), Some(3));
            v.jump_paragraph(&cs, false, 1); // to the top
            assert_eq!(v.cursor, 0);
            v.jump_paragraph(&cs, true, 2);
            assert_eq!(new_of(&v), Some(6));
        }

        #[test]
        fn find_line_exact_and_nearest() {
            let (_d, cs) = diff(OLD, "1\n3\n4\nfive\n6\n7\nnew\n8\n", 0);
            let page = DiffView::page(&cs, 0, None);
            let scroll = DiffView::scroll(&cs);
            // Unchanged line 6 (new 5) exists only in Page mode.
            let row = page.find_line(&cs, 0, Some(6), Some(5)).unwrap();
            assert_eq!(page.rows[row].line_numbers(&cs), Some((Some(6), Some(5))));
            // In Scroll mode, the nearest line is "+/4" (distance 1).
            let row = scroll.find_line(&cs, 0, Some(6), Some(5)).unwrap();
            assert_eq!(scroll.rows[row].line_numbers(&cs), Some((None, Some(4))));
            // Removed line is found by its old number.
            let row = page.find_line(&cs, 0, Some(2), None).unwrap();
            assert_eq!(page.rows[row].line_numbers(&cs), Some((Some(2), None)));
        }
    }
}
