//! Syntax highlighting via syntect with bat's syntax and theme sets.

use std::collections::HashMap;
use std::path::Path;

use anyhow::anyhow;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::model::{Changeset, Side};

/// Files larger than this are shown without highlighting: syntect is too slow on them.
const MAX_HIGHLIGHT_BYTES: usize = 1024 * 1024;
const TAB_WIDTH: usize = 4;

pub type StyledLine = Vec<Span<'static>>;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl Highlighter {
    pub fn new(theme_name: &str) -> anyhow::Result<Self> {
        let mut themes = ThemeSet::from(two_face::theme::extra());
        let theme = themes.themes.remove(theme_name).ok_or_else(|| {
            let mut names: Vec<_> = themes.themes.keys().cloned().collect();
            names.sort();
            anyhow!("unknown theme {theme_name:?}, available: {}", names.join(", "))
        })?;
        Ok(Self { syntaxes: two_face::syntax::extra_newlines(), theme })
    }

    /// Highlights the whole file. Returns `None` if the file type is unknown
    /// or the file is too large; callers then render plain text.
    pub fn highlight(&self, path: &Path, text: &str) -> Option<Vec<StyledLine>> {
        if text.len() > MAX_HIGHLIGHT_BYTES {
            return None;
        }
        let syntax = self.detect(path, text)?;
        let mut h = HighlightLines::new(syntax, &self.theme);
        let mut out = Vec::new();
        for line in LinesWithEndings::from(text) {
            let ranges = h.highlight_line(line, &self.syntaxes).ok()?;
            let spans = ranges
                .into_iter()
                .filter_map(|(style, piece)| {
                    let piece = piece.trim_end_matches(['\n', '\r']);
                    (!piece.is_empty()).then(|| Span::styled(expand_tabs(piece), convert(style)))
                })
                .collect();
            out.push(spans);
        }
        Some(out)
    }

    fn detect(&self, path: &Path, text: &str) -> Option<&SyntaxReference> {
        let by_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| self.syntaxes.find_syntax_by_extension(e));
        // Sublime syntaxes list some full names (Makefile, Dockerfile) as "extensions".
        let by_name = || {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| self.syntaxes.find_syntax_by_extension(n))
        };
        let by_first_line = || {
            text.lines()
                .next()
                .and_then(|l| self.syntaxes.find_syntax_by_first_line(l))
        };
        by_ext.or_else(by_name).or_else(by_first_line)
    }
}

fn convert(s: syntect::highlighting::Style) -> Style {
    let mut style = Style::default().fg(Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b));
    if s.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

/// Terminals render tabs inconsistently, so we expand them ourselves.
pub fn expand_tabs(s: &str) -> String {
    if s.contains('\t') { s.replace('\t', &" ".repeat(TAB_WIDTH)) } else { s.to_owned() }
}

/// Highlighted files, computed lazily when a file first becomes visible.
#[derive(Default)]
pub struct HighlightCache {
    files: HashMap<(usize, Side), Option<Vec<StyledLine>>>,
}

impl HighlightCache {
    pub fn clear(&mut self) {
        self.files.clear();
    }

    pub fn ensure(&mut self, hl: &Highlighter, cs: &Changeset, file: usize, side: Side) {
        self.files.entry((file, side)).or_insert_with(|| {
            let f = &cs.files[file];
            let text = f.content(side)?;
            let path = match side {
                Side::Old => f.old_path.as_deref(),
                Side::New => f.new_path.as_deref(),
            }?;
            hl.highlight(path, text.as_str())
        });
    }

    /// Highlighted line `lineno` (1-based) of the file, if it matches `expected` text.
    /// The check protects against the file changing on disk after the diff was taken.
    pub fn line(&self, file: usize, side: Side, lineno: u32, expected: &str) -> Option<&StyledLine> {
        let lines = self.files.get(&(file, side))?.as_ref()?;
        let line = lines.get(lineno.checked_sub(1)? as usize)?;
        let mut rest = expected;
        for span in line {
            rest = rest.strip_prefix(span.content.as_ref())?;
        }
        rest.is_empty().then_some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_exists() {
        Highlighter::new(&crate::config::UiConfig::default().theme).unwrap();
    }

    #[test]
    fn unknown_theme_is_error() {
        assert!(Highlighter::new("no-such-theme").is_err());
    }

    #[test]
    fn highlights_by_extension_and_name() {
        let h = Highlighter::new("base16-ocean.dark").unwrap();
        let lines = h.highlight(Path::new("x.rs"), "fn main() {\n\tlet x = 1;\n}\n").unwrap();
        assert_eq!(lines.len(), 3);
        let text: String = lines[1].iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "    let x = 1;");
        assert!(h.highlight(Path::new("x.ts"), "let a: number = 1;\n").is_some());
        assert!(h.highlight(Path::new("Cargo.toml"), "[package]\n").is_some());
        assert!(h.highlight(Path::new("Makefile"), "all:\n").is_some());
        assert!(h.highlight(Path::new("run"), "#!/bin/bash\necho\n").is_some());
    }
}
