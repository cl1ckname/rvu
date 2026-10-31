//! Single-line text input with insert-mode style editing keys.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, PartialEq, Eq)]
pub enum InputResult {
    Continue,
    Submit,
    Cancel,
}

#[derive(Debug, Default)]
pub struct LineEditor {
    chars: Vec<char>,
    /// Cursor position in chars, `0..=chars.len()`.
    cursor: usize,
}

impl LineEditor {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Display width of the text before the cursor.
    pub fn cursor_width(&self) -> usize {
        self.chars[..self.cursor].iter().collect::<String>().width()
    }

    pub fn handle(&mut self, key: KeyEvent) -> InputResult {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => return InputResult::Submit,
            KeyCode::Esc => return InputResult::Cancel,
            KeyCode::Char('c') if ctrl => return InputResult::Cancel,
            KeyCode::Char('w') if ctrl => self.delete_word(),
            KeyCode::Char('u') if ctrl => {
                self.chars.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('h') if ctrl => self.backspace(),
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.chars.len(),
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char(c) => {
                self.chars.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            _ => {}
        }
        InputResult::Continue
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    /// `Ctrl-w`: delete trailing spaces, then the word before the cursor.
    fn delete_word(&mut self) {
        let mut start = self.cursor;
        while start > 0 && self.chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !self.chars[start - 1].is_whitespace() {
            start -= 1;
        }
        self.chars.drain(start..self.cursor);
        self.cursor = start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn typed(s: &str) -> LineEditor {
        let mut e = LineEditor::default();
        for c in s.chars() {
            e.handle(key(KeyCode::Char(c)));
        }
        e
    }

    #[test]
    fn typing_and_editing() {
        let mut e = typed("fix thе bug");
        e.handle(key(KeyCode::Left));
        e.handle(key(KeyCode::Left));
        e.handle(key(KeyCode::Left));
        e.handle(key(KeyCode::Backspace));
        e.handle(key(KeyCode::Char('_')));
        assert_eq!(e.text(), "fix thе_bug");
        e.handle(ctrl('a'));
        e.handle(key(KeyCode::Delete));
        assert_eq!(e.text(), "ix thе_bug");
        assert_eq!(e.cursor_width(), 0);
    }

    #[test]
    fn delete_word_and_line() {
        let mut e = typed("FIXME: wrong  index  ");
        e.handle(ctrl('w'));
        assert_eq!(e.text(), "FIXME: wrong  ");
        e.handle(ctrl('w'));
        assert_eq!(e.text(), "FIXME: ");
        e.handle(ctrl('u'));
        assert_eq!(e.text(), "");
    }

    #[test]
    fn submit_and_cancel() {
        let mut e = typed("x");
        assert_eq!(e.handle(key(KeyCode::Enter)), InputResult::Submit);
        assert_eq!(e.handle(key(KeyCode::Esc)), InputResult::Cancel);
        assert_eq!(e.handle(ctrl('c')), InputResult::Cancel);
    }

    #[test]
    fn wide_chars_width() {
        let e = typed("日本");
        assert_eq!(e.cursor_width(), 4);
    }
}
