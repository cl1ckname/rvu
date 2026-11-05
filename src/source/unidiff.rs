//! Parser for unified diff text (as returned by the GitLab API).

use anyhow::{Context, anyhow};

use crate::model::{DiffLine, Hunk, LineKind};

/// Parses hunks from unified diff text. Lines before the first `@@`
/// (`diff --git`, `---`, `+++`) are ignored.
pub fn parse_hunks(text: &str) -> anyhow::Result<Vec<Hunk>> {
    let mut hunks: Vec<Hunk> = Vec::new();
    // Next line numbers within the current hunk.
    let (mut old_no, mut new_no) = (0u32, 0u32);
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with("@@") {
            let h = parse_header(line).with_context(|| format!("bad hunk header {line:?}"))?;
            old_no = h.old_start.max(1);
            new_no = h.new_start.max(1);
            hunks.push(h);
            continue;
        }
        let Some(hunk) = hunks.last_mut() else { continue };
        let (kind, content) = match line.split_at_checked(1) {
            Some((" ", rest)) => (LineKind::Context, rest),
            Some(("+", rest)) => (LineKind::Added, rest),
            Some(("-", rest)) => (LineKind::Removed, rest),
            // "\ No newline at end of file".
            Some(("\\", _)) => continue,
            // Some producers drop the space of empty context lines.
            None => (LineKind::Context, ""),
            Some(_) => continue,
        };
        let (old_lineno, new_lineno) = match kind {
            LineKind::Context => (Some(old_no), Some(new_no)),
            LineKind::Added => (None, Some(new_no)),
            LineKind::Removed => (Some(old_no), None),
        };
        old_no += old_lineno.is_some() as u32;
        new_no += new_lineno.is_some() as u32;
        hunk.lines.push(DiffLine { kind, old_lineno, new_lineno, content: content.to_owned() });
    }
    Ok(hunks)
}

/// `@@ -a[,b] +c[,d] @@ context`.
fn parse_header(line: &str) -> anyhow::Result<Hunk> {
    let rest = line.strip_prefix("@@ ").ok_or_else(|| anyhow!("no @@"))?;
    let (ranges, _context) = rest.split_once(" @@").ok_or_else(|| anyhow!("no closing @@"))?;
    let (old, new) = ranges.split_once(' ').ok_or_else(|| anyhow!("no ranges"))?;
    let range = |r: &str, sign: char| -> anyhow::Result<(u32, u32)> {
        let r = r.strip_prefix(sign).ok_or_else(|| anyhow!("range without {sign}"))?;
        Ok(match r.split_once(',') {
            Some((start, len)) => (start.parse()?, len.parse()?),
            None => (r.parse()?, 1),
        })
    };
    let (old_start, old_len) = range(old, '-')?;
    let (new_start, new_len) = range(new, '+')?;
    Ok(Hunk { header: line.to_owned(), old_start, old_len, new_start, new_len, lines: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunks_with_line_numbers() {
        let text = "--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@ fn main\n a\n-b\n+B\n c\n@@ -10 +10,2 @@\n x\n+y\n\\ No newline at end of file\n";
        let hunks = parse_hunks(text).unwrap();
        assert_eq!(hunks.len(), 2);
        let h = &hunks[0];
        assert_eq!((h.old_start, h.old_len, h.new_start, h.new_len), (1, 3, 1, 3));
        assert_eq!(h.header, "@@ -1,3 +1,3 @@ fn main");
        let lines: Vec<_> =
            h.lines.iter().map(|l| (l.kind, l.old_lineno, l.new_lineno, l.content.as_str())).collect();
        assert_eq!(
            lines,
            [
                (LineKind::Context, Some(1), Some(1), "a"),
                (LineKind::Removed, Some(2), None, "b"),
                (LineKind::Added, None, Some(2), "B"),
                (LineKind::Context, Some(3), Some(3), "c"),
            ]
        );
        let h = &hunks[1];
        assert_eq!((h.old_start, h.old_len, h.new_start, h.new_len), (10, 1, 10, 2));
        assert_eq!(h.lines.len(), 2);
        assert_eq!(h.lines[1].new_lineno, Some(11));
    }

    #[test]
    fn new_and_deleted_files() {
        let added = parse_hunks("@@ -0,0 +1,2 @@\n+a\n+b\n").unwrap();
        assert_eq!(added[0].lines[1].new_lineno, Some(2));
        let deleted = parse_hunks("@@ -1,2 +0,0 @@\n-a\n-b\n").unwrap();
        assert_eq!(deleted[0].lines[1].old_lineno, Some(2));
        assert!(parse_hunks("").unwrap().is_empty());
    }

    #[test]
    fn bad_header_is_error() {
        assert!(parse_hunks("@@ garbage\n").is_err());
    }
}
