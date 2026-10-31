//! Local repository via libgit2.

use std::path::{Path, PathBuf};

use anyhow::Context;
use git2::{Delta, DiffOptions, Patch, Repository};

use crate::model::{Changeset, DiffLine, FileDiff, FileStatus, Hunk, LineKind, Text};

pub struct LocalRepo {
    repo: Repository,
    workdir: PathBuf,
}

impl LocalRepo {
    /// Finds the repository containing `path` (like `git` does).
    pub fn discover(path: &Path) -> anyhow::Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("{} is not inside a git repository", path.display()))?;
        let workdir = repo
            .workdir()
            .context("bare repositories are not supported")?
            .to_path_buf();
        Ok(Self { repo, workdir })
    }

    /// Working tree vs index, like `git diff`.
    pub fn working_tree_diff(&self, context_lines: u32) -> anyhow::Result<Changeset> {
        let mut opts = DiffOptions::new();
        opts.context_lines(context_lines);
        let diff = self.repo.diff_index_to_workdir(None, Some(&mut opts))?;

        let mut files = Vec::new();
        for idx in 0..diff.deltas().len() {
            let Some(patch) = Patch::from_diff(&diff, idx)? else {
                continue;
            };
            files.push(self.file_diff(&patch)?);
        }
        Ok(Changeset { files })
    }

    fn file_diff(&self, patch: &Patch) -> anyhow::Result<FileDiff> {
        let delta = patch.delta();
        let status = match delta.status() {
            Delta::Added | Delta::Untracked => FileStatus::Added,
            Delta::Deleted => FileStatus::Deleted,
            Delta::Modified => FileStatus::Modified,
            Delta::Renamed => FileStatus::Renamed,
            _ => FileStatus::Other,
        };
        let old_path = (status != FileStatus::Added)
            .then(|| delta.old_file().path().map(Path::to_path_buf))
            .flatten();
        let new_path = (status != FileStatus::Deleted)
            .then(|| delta.new_file().path().map(Path::to_path_buf))
            .flatten();
        let binary = delta.flags().is_binary();

        let mut hunks = Vec::with_capacity(patch.num_hunks());
        if !binary {
            for h in 0..patch.num_hunks() {
                let (hunk, n_lines) = patch.hunk(h)?;
                let mut lines = Vec::with_capacity(n_lines);
                for l in 0..n_lines {
                    let line = patch.line_in_hunk(h, l)?;
                    let kind = match line.origin() {
                        ' ' => LineKind::Context,
                        '+' => LineKind::Added,
                        '-' => LineKind::Removed,
                        // "\ No newline at end of file" markers and the like.
                        _ => continue,
                    };
                    lines.push(DiffLine {
                        kind,
                        old_lineno: line.old_lineno(),
                        new_lineno: line.new_lineno(),
                        content: trim_eol(&String::from_utf8_lossy(line.content())).to_owned(),
                    });
                }
                hunks.push(Hunk {
                    header: trim_eol(&String::from_utf8_lossy(hunk.header())).to_owned(),
                    old_start: hunk.old_start(),
                    old_len: hunk.old_lines(),
                    new_start: hunk.new_start(),
                    new_len: hunk.new_lines(),
                    lines,
                });
            }
        }

        let (old_content, new_content) = if binary {
            (None, None)
        } else {
            let old = match &old_path {
                Some(_) if !delta.old_file().id().is_zero() => {
                    let blob = self.repo.find_blob(delta.old_file().id())?;
                    Some(Text::from(String::from_utf8_lossy(blob.content()).into_owned()))
                }
                _ => None,
            };
            let new = match &new_path {
                // The new side of `git diff` is the working tree itself.
                Some(p) => std::fs::read(self.workdir.join(p))
                    .ok()
                    .map(|b| Text::from(String::from_utf8_lossy(&b).into_owned())),
                None => None,
            };
            (old, new)
        };

        Ok(FileDiff { old_path, new_path, status, binary, hunks, old_content, new_content })
    }
}

fn trim_eol(s: &str) -> &str {
    s.strip_suffix('\n').map(|s| s.strip_suffix('\r').unwrap_or(s)).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Side;

    /// Creates a repo with `files` committed to the index.
    fn repo_with(files: &[(&str, &str)]) -> (tempfile::TempDir, LocalRepo) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut index = repo.index().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).unwrap();
            index.add_path(Path::new(name)).unwrap();
        }
        index.write().unwrap();
        let local = LocalRepo::discover(dir.path()).unwrap();
        (dir, local)
    }

    #[test]
    fn modified_file_has_hunks_and_line_numbers() {
        let (dir, repo) = repo_with(&[("a.txt", "one\ntwo\nthree\n")]);
        std::fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();

        let cs = repo.working_tree_diff(3).unwrap();
        assert_eq!(cs.files.len(), 1);
        let f = &cs.files[0];
        assert_eq!(f.status, FileStatus::Modified);
        assert_eq!(f.path(), Path::new("a.txt"));
        assert_eq!(f.content(Side::Old).unwrap().as_str(), "one\ntwo\nthree\n");
        assert_eq!(f.content(Side::New).unwrap().as_str(), "one\nTWO\nthree\nfour\n");
        let h = &f.hunks[0];
        assert_eq!((h.old_start, h.old_len, h.new_start, h.new_len), (1, 3, 1, 4));

        let lines: Vec<_> = f.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_lineno, l.new_lineno, l.content.as_str()))
            .collect();
        assert_eq!(
            lines,
            vec![
                (LineKind::Context, Some(1), Some(1), "one"),
                (LineKind::Removed, Some(2), None, "two"),
                (LineKind::Added, None, Some(2), "TWO"),
                (LineKind::Context, Some(3), Some(3), "three"),
                (LineKind::Added, None, Some(4), "four"),
            ]
        );
    }

    #[test]
    fn deleted_file() {
        let (dir, repo) = repo_with(&[("a.txt", "x\n"), ("b.txt", "y\n")]);
        std::fs::remove_file(dir.path().join("b.txt")).unwrap();

        let cs = repo.working_tree_diff(3).unwrap();
        assert_eq!(cs.files.len(), 1);
        assert_eq!(cs.files[0].status, FileStatus::Deleted);
        assert_eq!(cs.files[0].new_path, None);
        assert_eq!(cs.files[0].path(), Path::new("b.txt"));
    }

    #[test]
    fn clean_tree_is_empty() {
        let (_dir, repo) = repo_with(&[("a.txt", "x\n")]);
        assert!(repo.working_tree_diff(3).unwrap().files.is_empty());
    }
}
