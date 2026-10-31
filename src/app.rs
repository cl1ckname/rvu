use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use ratatui::{DefaultTerminal, Frame};

use crate::cli::Target;
use crate::config::{Config, ViewMode};
use crate::highlight::{HighlightCache, Highlighter};
use crate::keymap::{Action, DEFAULT_BINDINGS, Keymap};
use crate::model::{Changeset, LineKind, Side};
use crate::source::local::LocalRepo;
use crate::tree::{FileTree, ItemKind};
use crate::ui;
use crate::view::{DiffView, Row};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Tree,
    Diff,
}

/// Cursor position remembered across reloads: file path + (old, new) line numbers.
type Anchor = (std::path::PathBuf, Option<(Option<u32>, Option<u32>)>);

enum Message {
    Info(String),
    Error(String),
}

pub struct App {
    config: Config,
    target: Target,
    repo: Option<LocalRepo>,
    changeset: Changeset,
    view: DiffView,
    tree: FileTree,
    focus: Focus,
    keymap: Keymap,
    highlighter: Highlighter,
    highlights: HighlightCache,
    gutter_digits: usize,
    mode: ViewMode,
    /// File shown in Page mode.
    page_file: usize,
    /// Page mode: unchanged regions are folded.
    fold: bool,
    message: Option<Message>,
    show_tree: bool,
    show_help: bool,
    help_scroll: usize,
    should_quit: bool,
}

impl App {
    pub fn new(
        config: Config,
        target: Target,
        repo: Option<LocalRepo>,
        changeset: Changeset,
        highlighter: Highlighter,
    ) -> Self {
        let message = match target {
            Target::GitLabMr { .. } => {
                Some(Message::Error("GitLab merge requests are not supported yet".into()))
            }
            Target::WorkingTree => None,
        };
        let mode = config.ui.default_mode;
        let fold = config.page.fold_unchanged;
        let show_tree = config.ui.show_tree;
        let mut app = Self {
            config,
            target,
            repo,
            changeset: Changeset::default(),
            view: DiffView::default(),
            tree: FileTree::default(),
            focus: Focus::Diff,
            keymap: Keymap::new(DEFAULT_BINDINGS),
            highlighter,
            highlights: HighlightCache::default(),
            gutter_digits: 3,
            mode,
            page_file: 0,
            fold,
            message,
            show_tree,
            show_help: false,
            help_scroll: 0,
            should_quit: false,
        };
        app.set_changeset(changeset);
        app
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        let mut events = EventStream::new();
        while !self.should_quit {
            terminal.draw(|f| self.draw(f))?;
            match events.next().await {
                Some(Ok(event)) => self.handle_event(event),
                Some(Err(e)) => return Err(e.into()),
                None => break,
            }
        }
        Ok(())
    }

    fn set_changeset(&mut self, mut changeset: Changeset) {
        // Show files in the diff in the same order as in the tree.
        let order = FileTree::new(&changeset).file_order();
        let mut files: Vec<_> = changeset.files.into_iter().map(Some).collect();
        changeset.files = order.into_iter().map(|i| files[i].take().unwrap()).collect();

        // Stay on the same page if the file is still there.
        let page_path = self.changeset.files.get(self.page_file).map(|f| f.path().to_path_buf());
        self.page_file = page_path
            .and_then(|p| changeset.files.iter().position(|f| f.path() == p))
            .unwrap_or(self.page_file)
            .min(changeset.files.len().saturating_sub(1));

        self.tree = FileTree::new(&changeset);
        self.gutter_digits = ui::gutter_digits(&changeset);
        self.highlights.clear();
        self.changeset = changeset;
        self.rebuild_view();
    }

    /// Rebuilds rows for the current mode (and page). Cursor goes to the top.
    fn rebuild_view(&mut self) {
        let view = match self.mode {
            ViewMode::Scroll => DiffView::scroll(&self.changeset),
            ViewMode::Page if self.changeset.files.is_empty() => DiffView::default(),
            ViewMode::Page => DiffView::page(
                &self.changeset,
                self.page_file,
                self.fold.then_some(self.config.page.fold_context),
            ),
        };
        self.view = DiffView { height: self.view.height, ..view };
    }

    /// Page mode: shows `file`, cursor on its first (or last) change.
    fn open_page(&mut self, file: usize, last_change: bool) {
        self.page_file = file;
        self.rebuild_view();
        if let Some(row) = self.view.edge_change(&self.changeset, !last_change) {
            self.view.goto(row);
            self.view.cursor_to_center();
        }
    }

    fn toggle_mode(&mut self) {
        let anchor = self.anchor();
        if let Some(file) = self.view.current_file() {
            self.page_file = file;
        }
        self.mode = match self.mode {
            ViewMode::Scroll => ViewMode::Page,
            ViewMode::Page => ViewMode::Scroll,
        };
        self.rebuild_view();
        if self.goto_anchor(anchor) {
            self.view.cursor_to_center();
        }
    }

    fn set_fold(&mut self, fold: bool) {
        self.fold = fold;
        if self.mode == ViewMode::Page {
            let anchor = self.anchor();
            self.rebuild_view();
            if self.goto_anchor(anchor) {
                self.view.cursor_to_center();
            }
        }
        let state = if fold { "hidden" } else { "shown" };
        let note = if self.mode == ViewMode::Scroll { " (applies to Page mode)" } else { "" };
        self.message = Some(Message::Info(format!("Unchanged lines {state}{note}")));
    }

    /// Current position as file path + line numbers: survives reloads and mode switches.
    fn anchor(&self) -> Option<Anchor> {
        let row = self.view.current()?;
        let file = row.file()?;
        Some((self.changeset.files[file].path().to_path_buf(), row.line_numbers(&self.changeset)))
    }

    /// Moves the cursor to the anchored line (or the nearest one).
    /// Returns `false` if the file is no longer in the changeset.
    fn goto_anchor(&mut self, anchor: Option<Anchor>) -> bool {
        let Some((path, lines)) = anchor else { return false };
        let Some(file) = self.changeset.files.iter().position(|f| f.path() == path) else {
            return false;
        };
        if self.mode == ViewMode::Page && file != self.page_file {
            self.page_file = file;
            self.rebuild_view();
        }
        let row = match lines {
            Some((old, new)) => self.view.find_line(&self.changeset, file, old, new),
            None => self.view.file_row(file),
        };
        if let Some(row) = row {
            self.view.goto(row);
        }
        true
    }

    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }
        if self.show_help {
            self.handle_help_key(key);
            return;
        }
        self.message = None;
        if let Some((action, count)) = self.keymap.feed(key) {
            self.dispatch(action, count);
        }
    }

    /// Keys while the cheatsheet is open: scroll it or close it.
    fn handle_help_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
            KeyCode::Char('d') if ctrl => self.help_scroll += 10,
            KeyCode::Char('u') if ctrl => self.help_scroll = self.help_scroll.saturating_sub(10),
            KeyCode::Char('g') => self.help_scroll = 0,
            KeyCode::Char('G') => self.help_scroll = usize::MAX,
            KeyCode::Char('?' | 'q') | KeyCode::Esc => self.show_help = false,
            KeyCode::Char('c') if ctrl => self.show_help = false,
            _ => {}
        }
    }

    fn dispatch(&mut self, action: Action, count: usize) {
        let n = count as isize;
        match action {
            Action::Quit => self.should_quit = true,
            Action::ToggleHelp => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            Action::ToggleMode => self.toggle_mode(),
            Action::ToggleFold => self.set_fold(!self.fold),
            Action::FoldAll => self.set_fold(true),
            Action::UnfoldAll => self.set_fold(false),
            Action::FocusNext => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Diff,
                    Focus::Diff => Focus::Tree,
                };
                self.show_tree |= self.focus == Focus::Tree;
            }
            Action::FocusTree => {
                self.focus = Focus::Tree;
                self.show_tree = true;
            }
            Action::ToggleTree => {
                self.show_tree = !self.show_tree;
                // A hidden panel cannot keep focus; a shown one gets it.
                self.focus = if self.show_tree { Focus::Tree } else { Focus::Diff };
            }
            Action::FocusDiff => self.focus = Focus::Diff,
            Action::Refresh => self.refresh(),
            // Change/file jumps always act on the diff, whatever is focused.
            Action::NextChange => self.jump_change(true, count),
            Action::PrevChange => self.jump_change(false, count),
            Action::NextFile => self.jump_file(true, count),
            Action::PrevFile => self.jump_file(false, count),
            _ => match self.focus {
                Focus::Diff => self.dispatch_diff(action, n),
                Focus::Tree => self.dispatch_tree(action, n),
            },
        }
        if (self.focus == Focus::Diff || matches!(action, Action::NextFile | Action::PrevFile))
            && let Some(file) = self.view.current_file()
        {
            self.tree.select_file(file);
        }
    }

    fn jump_change(&mut self, forward: bool, count: usize) {
        let done = self.view.jump_change(&self.changeset, forward, count);
        // Page mode: past the last change of the file, continue in the next one.
        if done < count && self.mode == ViewMode::Page {
            let next = if forward { self.page_file.checked_add(1) } else { self.page_file.checked_sub(1) };
            if let Some(next) = next.filter(|&f| f < self.changeset.files.len()) {
                self.open_page(next, !forward);
            }
        }
    }

    fn jump_file(&mut self, forward: bool, count: usize) {
        match self.mode {
            ViewMode::Scroll => self.view.jump_file(forward, count),
            ViewMode::Page => {
                let last = self.changeset.files.len().saturating_sub(1);
                let target = if forward {
                    self.page_file.saturating_add(count).min(last)
                } else {
                    self.page_file.saturating_sub(count)
                };
                if target != self.page_file {
                    self.open_page(target, false);
                }
            }
        }
    }

    fn dispatch_diff(&mut self, action: Action, n: isize) {
        let half = (self.view.height / 2).max(1) as isize;
        let page = self.view.height.saturating_sub(2).max(1) as isize;
        let v = &mut self.view;
        match action {
            Action::Down => v.move_by(n),
            Action::Up => v.move_by(-n),
            Action::Top => v.goto(0),
            Action::Bottom => v.goto(usize::MAX),
            Action::HalfPageDown => v.scroll_by(half),
            Action::HalfPageUp => v.scroll_by(-half),
            Action::PageDown => v.scroll_by(page),
            Action::PageUp => v.scroll_by(-page),
            Action::ParagraphDown => v.jump_paragraph(&self.changeset, true, n as usize),
            Action::ParagraphUp => v.jump_paragraph(&self.changeset, false, n as usize),
            Action::CursorToCenter => v.cursor_to_center(),
            Action::CursorToTop => v.cursor_to_top(),
            Action::CursorToBottom => v.cursor_to_bottom(),
            _ => {}
        }
    }

    fn dispatch_tree(&mut self, action: Action, n: isize) {
        let half = (self.view.height / 2).max(1) as isize;
        let t = &mut self.tree;
        match action {
            Action::Down => t.move_by(n),
            Action::Up => t.move_by(-n),
            Action::Top => t.selected = 0,
            Action::Bottom => t.move_by(isize::MAX),
            Action::HalfPageDown | Action::PageDown => t.move_by(half),
            Action::HalfPageUp | Action::PageUp => t.move_by(-half),
            Action::Open => {
                if !t.set_expanded(None) {
                    self.focus = Focus::Diff;
                }
            }
            Action::Close => {
                if t.is_selected_expanded_dir() {
                    t.set_expanded(Some(false));
                } else {
                    t.select_parent();
                }
            }
            _ => {}
        }
        // Preview: the diff follows the file selected in the tree.
        let Some(ItemKind::File { file }) = self.tree.selected_item().map(|i| i.kind.clone()) else {
            return;
        };
        match self.mode {
            ViewMode::Page if file != self.page_file => self.open_page(file, false),
            ViewMode::Scroll if self.view.current_file() != Some(file) => {
                if let Some(row) = self.view.file_row(file) {
                    self.view.goto(row);
                    self.view.cursor_to_top();
                }
            }
            _ => {}
        }
    }

    fn refresh(&mut self) {
        let Some(repo) = &self.repo else { return };
        let anchor = self.anchor();
        let (offset, cursor) = (self.view.offset, self.view.cursor);

        match repo.working_tree_diff(self.config.ui.context_lines) {
            Ok(cs) => {
                let n = cs.files.len();
                self.set_changeset(cs);
                self.view.offset = offset;
                if !self.goto_anchor(anchor) {
                    self.view.goto(cursor);
                }
                self.message = Some(Message::Info(format!("Reloaded: {n} files changed")));
            }
            Err(e) => self.message = Some(Message::Error(format!("Reload failed: {e:#}"))),
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [body, status_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        let tree_width = if self.show_tree { self.config.ui.tree_width } else { 0 };
        let [tree_area, diff_area] =
            Layout::horizontal([Constraint::Length(tree_width), Constraint::Fill(1)]).areas(body);

        // Tree pane.
        if self.show_tree {
            let tree_block = ui::pane_block(
                Line::from(format!(" Files ({}) ", self.changeset.files.len())),
                self.focus == Focus::Tree,
            );
            let tree_inner = tree_block.inner(tree_area);
            tree_block.render(tree_area, frame.buffer_mut());
            self.tree.ensure_visible(tree_inner.height as usize);
            let focused = self.focus == Focus::Tree;
            ui::draw_tree(frame.buffer_mut(), tree_inner, &self.tree, &self.changeset, focused);
        }

        // Diff pane.
        let diff_block = ui::pane_block(self.diff_title(), self.focus == Focus::Diff);
        let diff_inner = diff_block.inner(diff_area);
        diff_block.render(diff_area, frame.buffer_mut());
        self.view.height = diff_inner.height as usize;
        self.view.ensure_visible();
        self.highlight_visible();
        ui::draw_diff(
            frame.buffer_mut(),
            diff_inner,
            &self.view,
            &self.changeset,
            &self.highlights,
            self.gutter_digits,
        );

        self.draw_status(frame, status_area);
        if self.show_help {
            let area = frame.area();
            self.help_scroll = ui::draw_help(frame.buffer_mut(), area, self.help_scroll);
        }
    }

    /// Highlights files that have lines in the viewport (lazily, once per file).
    fn highlight_visible(&mut self) {
        for idx in self.view.visible() {
            let (file, side) = match self.view.rows[idx] {
                Row::Code { file, hunk, line } => {
                    (file, self.changeset.files[file].hunks[hunk].lines[line].kind.side())
                }
                Row::FileLine { file, .. } => (file, Side::New),
                _ => continue,
            };
            self.highlights.ensure(&self.highlighter, &self.changeset, file, side);
        }
    }

    fn diff_title(&self) -> Line<'static> {
        let (added, removed) = (0..self.changeset.files.len())
            .map(|f| ui::file_stats(&self.changeset, f))
            .fold((0, 0), |(a, r), (fa, fr)| (a + fa, r + fr));
        let target = match &self.target {
            Target::WorkingTree => "working tree".to_owned(),
            Target::GitLabMr { url } => url.clone(),
        };
        Line::from(vec![
            format!(" {target} ").into(),
            Span::from(format!("+{added}")).fg(Color::Rgb(0x8f, 0xd1, 0x9e)),
            " ".into(),
            Span::from(format!("-{removed}")).fg(Color::Rgb(0xf0, 0x8a, 0x95)),
            " ".into(),
        ])
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let fold = if self.mode == ViewMode::Page && self.fold { "·FOLD" } else { "" };
        let mode = format!(" {:?}{fold} ", self.mode).to_uppercase();
        let mut left = vec![Span::from(mode).bold().reversed(), " ".into()];
        match &self.message {
            Some(Message::Error(e)) => left.push(Span::from(e.clone()).fg(Color::LightRed)),
            Some(Message::Info(i)) => left.push(Span::from(i.clone())),
            None => {
                if let Some(file) = self.view.current_file() {
                    let f = &self.changeset.files[file];
                    left.push(Span::from(f.path().display().to_string()));
                    left.push(format!("  [{}/{}]", file + 1, self.changeset.files.len()).dim());
                    if let Some(kind) = self.view.current_kind(&self.changeset)
                        && kind != LineKind::Context
                    {
                        left.push(format!("  {kind:?}").dim());
                    }
                }
            }
        }
        let right = format!("{}  ?: help ", self.keymap.pending_display());
        let [l, r] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(right.len() as u16)])
            .areas(area);
        frame.render_widget(Line::from(left), l);
        frame.render_widget(Line::from(right).dim(), r);
    }
}
