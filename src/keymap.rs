//! Vim-like key sequences with counts: `gg`, `]c`, `<C-w>l`, `5j`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    ToggleHelp,
    ToggleMode,
    Comment,
    ToggleFold,
    FoldAll,
    UnfoldAll,
    Down,
    Up,
    Top,
    Bottom,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    CursorToCenter,
    CursorToTop,
    CursorToBottom,
    ParagraphDown,
    ParagraphUp,
    NextChange,
    PrevChange,
    NextFile,
    PrevFile,
    ToggleTree,
    FocusNext,
    FocusTree,
    FocusDiff,
    Open,
    Close,
    Refresh,
}

impl Action {
    pub fn description(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::ToggleHelp => "show / hide this cheatsheet",
            Self::ToggleMode => "switch mode: scroll / page",
            Self::Comment => "comment the line (written into the file)",
            Self::ToggleFold => "page: hide / show unchanged lines",
            Self::FoldAll => "page: hide unchanged lines",
            Self::UnfoldAll => "page: show unchanged lines",
            Self::Down => "down",
            Self::Up => "up",
            Self::Top => "go to top",
            Self::Bottom => "go to bottom",
            Self::HalfPageDown => "half page down",
            Self::HalfPageUp => "half page up",
            Self::PageDown => "page down",
            Self::PageUp => "page up",
            Self::CursorToCenter => "put cursor line at center",
            Self::CursorToTop => "put cursor line at top",
            Self::CursorToBottom => "put cursor line at bottom",
            Self::ParagraphDown => "next blank line (paragraph)",
            Self::ParagraphUp => "previous blank line (paragraph)",
            Self::NextChange => "next change",
            Self::PrevChange => "previous change",
            Self::NextFile => "next file",
            Self::PrevFile => "previous file",
            Self::ToggleTree => "show / hide file tree",
            Self::FocusNext => "switch focus: files / diff",
            Self::FocusTree => "focus files",
            Self::FocusDiff => "focus diff",
            Self::Open => "open file / toggle directory",
            Self::Close => "collapse dir / parent / collapse all",
            Self::Refresh => "reload changes",
        }
    }
}

/// Cheatsheet layout: sections and the actions they list, in order.
pub const HELP_SECTIONS: &[(&str, &[Action])] = &[
    (
        "Move",
        &[
            Action::Down,
            Action::Up,
            Action::Top,
            Action::Bottom,
            Action::HalfPageDown,
            Action::HalfPageUp,
            Action::PageDown,
            Action::PageUp,
            Action::ParagraphDown,
            Action::ParagraphUp,
            Action::CursorToCenter,
            Action::CursorToTop,
            Action::CursorToBottom,
        ],
    ),
    (
        "Review",
        &[
            Action::Comment,
            Action::NextChange,
            Action::PrevChange,
            Action::NextFile,
            Action::PrevFile,
            Action::ToggleMode,
            Action::ToggleFold,
            Action::FoldAll,
            Action::UnfoldAll,
            Action::Refresh,
        ],
    ),
    (
        "Panes & file tree",
        &[Action::ToggleTree, Action::FocusNext, Action::FocusTree, Action::FocusDiff, Action::Open, Action::Close],
    ),
    ("General", &[Action::ToggleHelp, Action::Quit]),
];

/// Bindings that work but are not worth the space in the cheatsheet.
const HIDDEN_IN_HELP: &[&str] = &["<C-w><C-w>"];

/// Human-readable keys bound to `action`: `"j / ↓"`, `"Ctrl-w l"`.
pub fn keys_for(bindings: &[(&str, Action)], action: Action) -> String {
    bindings
        .iter()
        .filter(|(seq, a)| *a == action && !HIDDEN_IN_HELP.contains(seq))
        .map(|(seq, _)| pretty_sequence(seq))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn pretty_sequence(seq: &str) -> String {
    let keys = parse_sequence(seq);
    let names: Vec<String> = keys.iter().map(|k| k.pretty()).collect();
    // "gg", "]c" read better without spaces; "Ctrl-w l", "Space m" need them.
    if names.iter().all(|n| n.chars().count() == 1) { names.concat() } else { names.join(" ") }
}

/// Default bindings, in the order they are shown in help.
pub const DEFAULT_BINDINGS: &[(&str, Action)] = &[
    ("j", Action::Down),
    ("<Down>", Action::Down),
    ("k", Action::Up),
    ("<Up>", Action::Up),
    ("gg", Action::Top),
    ("G", Action::Bottom),
    ("<C-d>", Action::HalfPageDown),
    ("<C-u>", Action::HalfPageUp),
    ("<C-f>", Action::PageDown),
    ("<PageDown>", Action::PageDown),
    ("<PageUp>", Action::PageUp),
    ("zz", Action::CursorToCenter),
    ("zt", Action::CursorToTop),
    ("zb", Action::CursorToBottom),
    ("}", Action::ParagraphDown),
    ("{", Action::ParagraphUp),
    ("]c", Action::NextChange),
    ("[c", Action::PrevChange),
    ("]f", Action::NextFile),
    ("[f", Action::PrevFile),
    ("<C-b>", Action::ToggleTree),
    ("<Tab>", Action::FocusNext),
    ("<C-w>w", Action::FocusNext),
    ("<C-w><C-w>", Action::FocusNext),
    ("<C-w>h", Action::FocusTree),
    ("<C-w>l", Action::FocusDiff),
    ("l", Action::Open),
    ("<CR>", Action::Open),
    ("h", Action::Close),
    ("c", Action::Comment),
    ("<Space>m", Action::ToggleMode),
    ("zi", Action::ToggleFold),
    ("zM", Action::FoldAll),
    ("zR", Action::UnfoldAll),
    ("R", Action::Refresh),
    ("?", Action::ToggleHelp),
    ("q", Action::Quit),
    ("<C-c>", Action::Quit),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    code: KeyCode,
    ctrl: bool,
}

impl Key {
    fn from_event(ev: KeyEvent) -> Self {
        // Shift is already reflected in the char itself ('G', '?').
        Self { code: ev.code, ctrl: ev.modifiers.contains(KeyModifiers::CONTROL) }
    }

    fn pretty(self) -> String {
        let base = match self.code {
            KeyCode::Char(' ') => "Space".to_owned(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_owned(),
            KeyCode::Down => "↓".to_owned(),
            KeyCode::Up => "↑".to_owned(),
            KeyCode::PageDown => "PgDn".to_owned(),
            KeyCode::PageUp => "PgUp".to_owned(),
            other => format!("{other:?}"),
        };
        if self.ctrl { format!("Ctrl-{base}") } else { base }
    }

    fn display(self) -> String {
        let base = match self.code {
            KeyCode::Char(' ') => "<Space>".to_owned(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "<CR>".to_owned(),
            KeyCode::Tab => "<Tab>".to_owned(),
            other => format!("<{other:?}>"),
        };
        if self.ctrl {
            format!("<C-{}>", base.trim_matches(['<', '>']))
        } else {
            base
        }
    }
}

/// Parses `"gg"`, `"<C-w>l"`, `"<CR>"` into keys.
fn parse_sequence(s: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
            && end > 1
        {
            keys.push(parse_special(&rest[1..end]));
            rest = &rest[end + 1..];
        } else {
            keys.push(Key { code: KeyCode::Char(c), ctrl: false });
            rest = &rest[c.len_utf8()..];
        }
    }
    keys
}

fn parse_special(name: &str) -> Key {
    if let Some(inner) = name.strip_prefix("C-") {
        let mut key = parse_special(inner);
        key.ctrl = true;
        return key;
    }
    let code = match name {
        "CR" | "Enter" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "Esc" => KeyCode::Esc,
        "Space" => KeyCode::Char(' '),
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        single if single.chars().count() == 1 => KeyCode::Char(single.chars().next().unwrap()),
        other => panic!("unknown key name <{other}> in bindings"),
    };
    Key { code, ctrl: false }
}

pub struct Keymap {
    bindings: Vec<(Vec<Key>, Action)>,
    pending: Vec<Key>,
    count: Option<usize>,
}

impl Keymap {
    pub fn new(bindings: &[(&str, Action)]) -> Self {
        Self {
            bindings: bindings.iter().map(|(s, a)| (parse_sequence(s), *a)).collect(),
            pending: Vec::new(),
            count: None,
        }
    }

    /// Feeds a key press. Returns an action and its count (1 if not given)
    /// once a complete sequence is typed.
    pub fn feed(&mut self, ev: KeyEvent) -> Option<(Action, usize)> {
        let key = Key::from_event(ev);

        if key.code == KeyCode::Esc && !key.ctrl {
            self.reset();
            return None;
        }
        if self.pending.is_empty()
            && !key.ctrl
            && let KeyCode::Char(c @ '0'..='9') = key.code
            && (c != '0' || self.count.is_some())
        {
            let digit = c.to_digit(10).unwrap() as usize;
            self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(digit));
            return None;
        }

        self.pending.push(key);
        if let Some(action) = self.exact_match() {
            let count = self.count.unwrap_or(1);
            self.reset();
            return Some((action, count));
        }
        if self.is_prefix() {
            return None;
        }
        // Dead end: drop the sequence, but let the last key start a new one.
        let retry = self.pending.len() > 1;
        self.reset();
        if retry { self.feed(ev) } else { None }
    }

    /// What has been typed so far, like vim's `showcmd`.
    pub fn pending_display(&self) -> String {
        let mut s = self.count.map(|c| c.to_string()).unwrap_or_default();
        for k in &self.pending {
            s.push_str(&k.display());
        }
        s
    }

    fn exact_match(&self) -> Option<Action> {
        self.bindings.iter().find(|(seq, _)| *seq == self.pending).map(|(_, a)| *a)
    }

    fn is_prefix(&self) -> bool {
        self.bindings.iter().any(|(seq, _)| seq.starts_with(&self.pending))
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.count = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(km: &mut Keymap, keys: &str) -> Option<(Action, usize)> {
        let mut last = None;
        for key in parse_sequence(keys) {
            let mods = if key.ctrl { KeyModifiers::CONTROL } else { KeyModifiers::NONE };
            last = km.feed(KeyEvent::new(key.code, mods));
        }
        last
    }

    fn km() -> Keymap {
        Keymap::new(DEFAULT_BINDINGS)
    }

    #[test]
    fn single_and_multi_key() {
        assert_eq!(press(&mut km(), "j"), Some((Action::Down, 1)));
        assert_eq!(press(&mut km(), "gg"), Some((Action::Top, 1)));
        assert_eq!(press(&mut km(), "]c"), Some((Action::NextChange, 1)));
        assert_eq!(press(&mut km(), "<C-w>l"), Some((Action::FocusDiff, 1)));
        assert_eq!(press(&mut km(), "<C-d>"), Some((Action::HalfPageDown, 1)));
    }

    #[test]
    fn counts() {
        assert_eq!(press(&mut km(), "5j"), Some((Action::Down, 5)));
        assert_eq!(press(&mut km(), "12]c"), Some((Action::NextChange, 12)));
        assert_eq!(press(&mut km(), "10k"), Some((Action::Up, 10)));
    }

    #[test]
    fn pending_and_dead_ends() {
        let mut k = km();
        assert_eq!(press(&mut k, "3g"), None);
        assert_eq!(k.pending_display(), "3g");
        // "gj" is not bound: "g" is dropped, "j" is executed.
        assert_eq!(press(&mut k, "j"), Some((Action::Down, 1)));
        assert_eq!(k.pending_display(), "");
    }

    #[test]
    fn every_bound_action_is_in_cheatsheet() {
        for (seq, action) in DEFAULT_BINDINGS {
            assert!(
                HELP_SECTIONS.iter().any(|(_, actions)| actions.contains(action)),
                "{seq} -> {action:?} is missing from HELP_SECTIONS"
            );
        }
    }

    #[test]
    fn pretty_keys() {
        assert_eq!(keys_for(DEFAULT_BINDINGS, Action::Down), "j / ↓");
        assert_eq!(keys_for(DEFAULT_BINDINGS, Action::Top), "gg");
        assert_eq!(keys_for(DEFAULT_BINDINGS, Action::FocusDiff), "Ctrl-w l");
        assert_eq!(keys_for(DEFAULT_BINDINGS, Action::ToggleMode), "Space m");
        assert_eq!(keys_for(DEFAULT_BINDINGS, Action::HalfPageDown), "Ctrl-d");
    }

    #[test]
    fn escape_cancels() {
        let mut k = km();
        press(&mut k, "5g");
        assert_eq!(press(&mut k, "<Esc>"), None);
        assert_eq!(press(&mut k, "j"), Some((Action::Down, 1)));
    }
}
