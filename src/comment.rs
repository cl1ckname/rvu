//! Local comments: written straight into the working tree file
//! (see docs/adr/0005-local-comments-in-file.md).

use std::path::Path;

use anyhow::{Context, bail};

use crate::config::{CommentSyntax, CommentsConfig};

enum Token {
    Line(&'static str),
    Block(&'static str, &'static str),
}

use Token::{Block, Line};

/// Comment syntax by syntect syntax name (bat's syntax set).
/// Keyed by syntax, not extension, so that detection by file name and
/// shebang (used for highlighting) works for comments too.
const BY_SYNTAX: &[(&str, Token)] = &[
    // C family and friends.
    ("ActionScript", Line("//")),
    ("C", Line("//")),
    ("C#", Line("//")),
    ("C++", Line("//")),
    ("CMake C Header", Line("//")),
    ("CMake C++ Header", Line("//")),
    ("D", Line("//")),
    ("Dart", Line("//")),
    ("F#", Line("//")),
    ("GLSL", Line("//")),
    ("Go", Line("//")),
    ("Gomod", Line("//")),
    ("Graphviz (DOT)", Line("//")),
    ("Groovy", Line("//")),
    ("Java", Line("//")),
    ("JavaScript", Line("//")),
    ("JavaScript (Babel)", Line("//")),
    ("JavaScript (Rails)", Line("//")),
    ("jsonnet", Line("//")),
    ("Kotlin", Line("//")),
    ("Less", Line("//")),
    ("Objective-C", Line("//")),
    ("Objective-C++", Line("//")),
    ("Odin", Line("//")),
    ("Pascal", Line("//")),
    ("PHP", Line("//")),
    ("Protocol Buffer", Line("//")),
    ("QML", Line("//")),
    ("Rust", Line("//")),
    ("Sass", Line("//")),
    ("Scala", Line("//")),
    ("SCSS", Line("//")),
    ("Solidity", Line("//")),
    ("Stylus", Line("//")),
    ("Swift", Line("//")),
    ("SystemVerilog", Line("//")),
    ("TypeScript", Line("//")),
    ("TypeScriptReact", Line("//")),
    ("Typst", Line("//")),
    ("Verilog", Line("//")),
    ("WGSL", Line("//")),
    ("Zig", Line("//")),
    ("AsciiDoc (Asciidoctor)", Line("//")),
    // Hash comments: scripting languages and configs.
    ("Apache Conf", Line("#")),
    ("Authorized Keys", Line("#")),
    ("AWK", Line("#")),
    ("Bourne Again Shell (bash)", Line("#")),
    ("CMake", Line("#")),
    ("CoffeeScript", Line("#")),
    ("Crontab", Line("#")),
    ("Crystal", Line("#")),
    ("Dockerfile", Line("#")),
    ("Dockerfile (with bash)", Line("#")),
    ("DotENV", Line("#")),
    ("Elixir", Line("#")),
    ("Fish", Line("#")),
    ("GDScript (Godot Engine)", Line("#")),
    ("Git Attributes", Line("#")),
    ("Git Config", Line("#")),
    ("Git Ignore", Line("#")),
    ("Git Mailmap", Line("#")),
    ("gnuplot", Line("#")),
    ("GraphQL", Line("#")),
    ("Hosts File", Line("#")),
    ("INI", Line("#")),
    ("Java Properties", Line("#")),
    ("JQ", Line("#")),
    ("Julia", Line("#")),
    ("LiveScript", Line("#")),
    ("Makefile", Line("#")),
    ("nginx", Line("#")),
    ("Nim", Line("#")),
    ("Ninja", Line("#")),
    ("Nix", Line("#")),
    ("orgmode", Line("#")),
    ("Perl", Line("#")),
    ("PowerShell", Line("#")),
    ("Protocol Buffer (TEXT)", Line("#")),
    ("Puppet", Line("#")),
    ("Python", Line("#")),
    ("R", Line("#")),
    ("Rego", Line("#")),
    ("Requirements.txt", Line("#")),
    ("Robot Framework", Line("#")),
    ("Ruby", Line("#")),
    ("Ruby on Rails", Line("#")),
    ("Salt State (SLS)", Line("#")),
    ("SSH Config", Line("#")),
    ("SSHD Config", Line("#")),
    ("Tcl", Line("#")),
    ("Terraform", Line("#")),
    ("TOML", Line("#")),
    ("Vyper", Line("#")),
    ("YAML", Line("#")),
    // Double dash.
    ("Ada", Line("--")),
    ("Cabal", Line("--")),
    ("Elm", Line("--")),
    ("Haskell", Line("--")),
    ("Idris", Line("--")),
    ("Lean 4", Line("--")),
    ("Lua", Line("--")),
    ("PureScript", Line("--")),
    ("SQL", Line("--")),
    ("SQL (Rails)", Line("--")),
    ("VHDL", Line("--")),
    // Semicolon: Lisps and assemblers.
    ("Clojure", Line(";")),
    ("Lisp", Line(";")),
    ("LLVM", Line(";")),
    ("NSIS", Line(";")),
    ("Racket", Line(";")),
    ("x86_64 Assembly", Line(";")),
    // Percent.
    ("Erlang", Line("%")),
    ("LaTeX", Line("%")),
    ("MATLAB", Line("%")),
    ("TeX", Line("%")),
    // Others.
    ("Batch File", Line("::")),
    ("Fortran (Modern)", Line("!")),
    ("reStructuredText", Line("..")),
    ("VimL", Line("\"")),
    ("CSS", Block("/*", "*/")),
    ("OCaml", Block("(*", "*)")),
    ("SML", Block("(*", "*)")),
    ("Jinja2", Block("{#", "#}")),
    ("HTML", Block("<!--", "-->")),
    ("HTML (EEx)", Block("<!--", "-->")),
    ("HTML (Jinja2)", Block("<!--", "-->")),
    ("HTML (Rails)", Block("<!--", "-->")),
    ("HTML (Twig)", Block("<!--", "-->")),
    ("Markdown", Block("<!--", "-->")),
    ("MediaWiki", Block("<!--", "-->")),
    ("MultiMarkdown", Block("<!--", "-->")),
    ("Svelte", Block("<!--", "-->")),
    ("Vue Component", Block("<!--", "-->")),
    ("XML", Block("<!--", "-->")),
];

/// Comment syntax for a file: config override by extension (or full file
/// name for extensionless files like `Dockerfile`), then the built-in table.
pub fn syntax_for(config: &CommentsConfig, path: &Path, syntax_name: Option<&str>) -> Option<CommentSyntax> {
    let keys = [
        path.extension().and_then(|e| e.to_str()),
        path.file_name().and_then(|n| n.to_str()),
    ];
    if let Some(s) = keys.into_iter().flatten().find_map(|k| config.syntax.get(k)) {
        return Some(s.clone());
    }
    let name = syntax_name?;
    BY_SYNTAX.iter().find(|(n, _)| *n == name).map(|(_, t)| match t {
        Line(l) => CommentSyntax::Line((*l).to_owned()),
        Block(s, e) => CommentSyntax::Block([(*s).to_owned(), (*e).to_owned()]),
    })
}

/// `    // text` or `    <!-- text -->`.
pub fn format_line(syntax: &CommentSyntax, indent: &str, text: &str) -> String {
    match syntax {
        CommentSyntax::Line(token) => format!("{indent}{token} {text}"),
        CommentSyntax::Block([start, end]) => format!("{indent}{start} {text} {end}"),
    }
}

/// What goes before the user's text in the input line: `// ` or `<!-- `.
pub fn prompt_prefix(syntax: &CommentSyntax) -> String {
    match syntax {
        CommentSyntax::Line(token) => format!("{token} "),
        CommentSyntax::Block([start, _]) => format!("{start} "),
    }
}

/// Leading whitespace of a line.
pub fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Inserts `line` before line `before` (1-based) of `text`; past the end it
/// is appended. Keeps the file's line ending style (LF / CRLF).
pub fn insert_before(text: &str, before: u32, line: &str) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut offset = 0;
    for _ in 1..before {
        match text[offset..].find('\n') {
            Some(i) => offset += i + 1,
            None => {
                offset = text.len();
                break;
            }
        }
    }
    let mut out = String::with_capacity(text.len() + line.len() + 2);
    out.push_str(&text[..offset]);
    if offset == text.len() && !text.is_empty() && !text.ends_with('\n') {
        // Appending to a file without a trailing newline.
        out.push_str(eol);
        out.push_str(line);
        return out;
    }
    out.push_str(line);
    out.push_str(eol);
    out.push_str(&text[offset..]);
    out
}

/// Inserts the comment into the file on disk, but only if the file still
/// has exactly the contents the diff was built from.
pub fn write_comment(path: &Path, snapshot: &str, before: u32, line: &str) -> anyhow::Result<()> {
    let current = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    if current != snapshot.as_bytes() {
        bail!("{} changed on disk since the diff was loaded, reload with R", path.display());
    }
    std::fs::write(path, insert_before(snapshot, before, line))
        .with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_entry_is_a_real_syntax() {
        let set = two_face::syntax::extra_newlines();
        for (name, _) in BY_SYNTAX {
            assert!(set.find_syntax_by_name(name).is_some(), "unknown syntax name {name:?}");
        }
    }

    #[test]
    fn config_override_wins() {
        let mut config = CommentsConfig::default();
        config.syntax.insert("h".into(), CommentSyntax::Block(["/*".into(), "*/".into()]));
        config.syntax.insert("Justfile".into(), CommentSyntax::Line("#".into()));
        let c = |p: &str, s: Option<&str>| syntax_for(&config, Path::new(p), s);
        assert_eq!(c("a/x.h", Some("C")), Some(CommentSyntax::Block(["/*".into(), "*/".into()])));
        assert_eq!(c("x.c", Some("C")), Some(CommentSyntax::Line("//".into())));
        assert_eq!(c("Justfile", None), Some(CommentSyntax::Line("#".into())));
        assert_eq!(c("x.json", Some("JSON")), None);
        assert_eq!(c("x.unknown", None), None);
    }

    #[test]
    fn formatting() {
        let line = CommentSyntax::Line("//".into());
        let block = CommentSyntax::Block(["<!--".into(), "-->".into()]);
        assert_eq!(format_line(&line, "    ", "FIXME: x"), "    // FIXME: x");
        assert_eq!(format_line(&block, "\t", "todo"), "\t<!-- todo -->");
        assert_eq!(indent_of("\t  let x = 1;"), "\t  ");
        assert_eq!(indent_of("x"), "");
    }

    #[test]
    fn insertion() {
        assert_eq!(insert_before("a\nb\nc\n", 2, "X"), "a\nX\nb\nc\n");
        assert_eq!(insert_before("a\nb\n", 1, "X"), "X\na\nb\n");
        assert_eq!(insert_before("a\nb\n", 3, "X"), "a\nb\nX\n");
        assert_eq!(insert_before("a\nb\n", 99, "X"), "a\nb\nX\n");
        assert_eq!(insert_before("a\nb", 99, "X"), "a\nb\nX");
        assert_eq!(insert_before("a\r\nb\r\n", 2, "X"), "a\r\nX\r\nb\r\n");
        assert_eq!(insert_before("", 1, "X"), "X\n");
    }

    #[test]
    fn write_refuses_changed_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.rs");
        std::fs::write(&p, "a\nb\n").unwrap();
        write_comment(&p, "a\nb\n", 2, "// X").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "a\n// X\nb\n");
        assert!(write_comment(&p, "a\nb\n", 2, "// Y").is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "a\n// X\nb\n");
    }
}
