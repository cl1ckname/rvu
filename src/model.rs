//! Source-independent representation of a set of changes.

use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Changeset {
    pub files: Vec<FileDiff>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Other,
}

impl FileStatus {
    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Modified => 'M',
            Self::Renamed => 'R',
            Self::Other => '?',
        }
    }
}

#[derive(Debug)]
pub struct FileDiff {
    /// `None` for added files.
    pub old_path: Option<PathBuf>,
    /// `None` for deleted files.
    pub new_path: Option<PathBuf>,
    pub status: FileStatus,
    pub binary: bool,
    /// The source refused to produce a diff (GitLab: file too large).
    pub too_large: bool,
    pub hunks: Vec<Hunk>,
    /// Full old version of the file (for highlighting and Page mode).
    pub old_content: Option<Text>,
    /// Full new version of the file.
    pub new_content: Option<Text>,
}

impl FileDiff {
    /// Path to show to the user: new path, or old one for deleted files.
    pub fn path(&self) -> &Path {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .expect("file diff without paths")
    }

    pub fn content(&self, side: Side) -> Option<&Text> {
        match side {
            Side::Old => self.old_content.as_ref(),
            Side::New => self.new_content.as_ref(),
        }
    }
}

/// File contents with an index of line starts for O(1) access by line number.
#[derive(Debug)]
pub struct Text {
    raw: String,
    /// Byte offset of the start of each line.
    starts: Vec<usize>,
}

impl From<String> for Text {
    fn from(raw: String) -> Self {
        let mut starts = vec![0];
        starts.extend(raw.match_indices('\n').map(|(i, _)| i + 1));
        // A trailing newline does not start a new line.
        if starts.last() == Some(&raw.len()) {
            starts.pop();
        }
        Self { raw, starts }
    }
}

impl Text {
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }

    /// Line `n` (1-based) without the line terminator.
    pub fn line(&self, n: u32) -> Option<&str> {
        let i = n.checked_sub(1)? as usize;
        let start = *self.starts.get(i)?;
        let end = self.starts.get(i + 1).copied().unwrap_or(self.raw.len());
        let line = &self.raw[start..end];
        Some(line.strip_suffix('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).unwrap_or(line))
    }
}

#[derive(Debug)]
pub struct Hunk {
    /// `@@ -a,b +c,d @@ context` line, without trailing newline.
    pub header: String,
    pub old_start: u32,
    pub old_len: u32,
    /// For hunks that only remove lines (`new_len == 0`) this is the line
    /// *after which* the removal happened, 0 for the start of the file.
    pub new_start: u32,
    pub new_len: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

impl LineKind {
    pub fn is_change(self) -> bool {
        self != Self::Context
    }

    pub fn sign(self) -> char {
        match self {
            Self::Context => ' ',
            Self::Added => '+',
            Self::Removed => '-',
        }
    }

    /// Which version of the file this line belongs to.
    /// Context lines exist in both; we take them from the new one.
    pub fn side(self) -> Side {
        match self {
            Self::Removed => Side::Old,
            Self::Context | Self::Added => Side::New,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Old,
    New,
}

#[derive(Debug)]
pub struct DiffLine {
    pub kind: LineKind,
    /// 1-based line number in the old version.
    pub old_lineno: Option<u32>,
    /// 1-based line number in the new version.
    pub new_lineno: Option<u32>,
    /// Line text without the trailing newline.
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_lines() {
        let t = Text::from("a\nbb\r\n\nc".to_owned());
        assert_eq!(t.line_count(), 4);
        assert_eq!([t.line(1), t.line(2), t.line(3), t.line(4)], [Some("a"), Some("bb"), Some(""), Some("c")]);
        assert_eq!(t.line(0), None);
        assert_eq!(t.line(5), None);

        let t = Text::from("x\n".to_owned());
        assert_eq!(t.line_count(), 1);
        assert_eq!(Text::from(String::new()).line_count(), 0);
    }
}
