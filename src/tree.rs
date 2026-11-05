//! File tree of the changeset (left panel).

use std::collections::{BTreeMap, HashSet};

use crate::model::Changeset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// `end` is the index after the last item of this directory's subtree.
    Dir { end: usize },
    File { file: usize },
}

#[derive(Debug, Clone)]
pub struct Item {
    pub depth: usize,
    /// Directory chains with a single child are merged: `src/ui`.
    pub label: String,
    pub kind: ItemKind,
}

#[derive(Debug, Default)]
pub struct FileTree {
    /// All items in pre-order.
    items: Vec<Item>,
    collapsed: HashSet<usize>,
    /// Indices into `items` of currently visible entries.
    visible: Vec<usize>,
    /// Position in `visible`.
    pub selected: usize,
    pub offset: usize,
}

#[derive(Default)]
struct DirNode {
    dirs: BTreeMap<String, DirNode>,
    files: Vec<(String, usize)>,
}

impl FileTree {
    pub fn new(cs: &Changeset) -> Self {
        let mut root = DirNode::default();
        for (fi, f) in cs.files.iter().enumerate() {
            let parts: Vec<String> = f
                .path()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            let (name, dirs) = parts.split_last().expect("empty path");
            let mut node = &mut root;
            for d in dirs {
                node = node.dirs.entry(d.clone()).or_default();
            }
            node.files.push((name.clone(), fi));
        }
        let mut items = Vec::new();
        flatten(&root, 0, &mut items);
        let mut tree = Self { items, ..Self::default() };
        tree.rebuild_visible();
        tree
    }

    fn rebuild_visible(&mut self) {
        self.visible.clear();
        let mut i = 0;
        while i < self.items.len() {
            self.visible.push(i);
            i = match self.items[i].kind {
                ItemKind::Dir { end } if self.collapsed.contains(&i) => end,
                _ => i + 1,
            };
        }
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
    }

    /// File indices in tree order (directories first, then files).
    pub fn file_order(&self) -> Vec<usize> {
        self.items
            .iter()
            .filter_map(|i| match i.kind {
                ItemKind::File { file } => Some(file),
                ItemKind::Dir { .. } => None,
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.visible.len()
    }

    /// Visible items with their "is collapsed" flag.
    pub fn visible_items(&self) -> impl Iterator<Item = (&Item, bool)> {
        self.visible.iter().map(|&i| (&self.items[i], self.collapsed.contains(&i)))
    }

    pub fn selected_item(&self) -> Option<&Item> {
        self.visible.get(self.selected).map(|&i| &self.items[i])
    }

    pub fn move_by(&mut self, delta: isize) {
        self.selected = self.selected.saturating_add_signed(delta).min(self.len().saturating_sub(1));
    }

    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    /// Expands (`expand = Some(true)`), collapses or toggles the selected directory.
    /// Returns `false` if the selection is not a directory.
    pub fn set_expanded(&mut self, expand: Option<bool>) -> bool {
        let Some(&idx) = self.visible.get(self.selected) else { return false };
        if !matches!(self.items[idx].kind, ItemKind::Dir { .. }) {
            return false;
        }
        let collapse = expand.map(|e| !e).unwrap_or(!self.collapsed.contains(&idx));
        if collapse {
            self.collapsed.insert(idx);
        } else {
            self.collapsed.remove(&idx);
        }
        self.rebuild_visible();
        true
    }

    pub fn is_selected_expanded_dir(&self) -> bool {
        let Some(&idx) = self.visible.get(self.selected) else { return false };
        matches!(self.items[idx].kind, ItemKind::Dir { .. }) && !self.collapsed.contains(&idx)
    }

    /// Selects the parent directory of the current item.
    pub fn select_parent(&mut self) {
        let Some(&idx) = self.visible.get(self.selected) else { return };
        let depth = self.items[idx].depth;
        if let Some(pos) = self.visible[..self.selected]
            .iter()
            .rposition(|&i| self.items[i].depth < depth)
        {
            self.selected = pos;
        }
    }

    /// Selects the item for `file` if it is visible.
    pub fn select_file(&mut self, file: usize) {
        if let Some(pos) = self
            .visible
            .iter()
            .position(|&i| self.items[i].kind == ItemKind::File { file })
        {
            self.selected = pos;
        }
    }
}

fn flatten(node: &DirNode, depth: usize, out: &mut Vec<Item>) {
    for (name, child) in &node.dirs {
        // Merge single-directory chains: a/b/c -> "a/b/c".
        let mut label = name.clone();
        let mut child = child;
        while child.files.is_empty() && child.dirs.len() == 1 {
            let (n, c) = child.dirs.iter().next().unwrap();
            label = format!("{label}/{n}");
            child = c;
        }
        let idx = out.len();
        out.push(Item { depth, label, kind: ItemKind::Dir { end: 0 } });
        flatten(child, depth + 1, out);
        out[idx].kind = ItemKind::Dir { end: out.len() };
    }
    for (name, fi) in &node.files {
        out.push(Item { depth, label: name.clone(), kind: ItemKind::File { file: *fi } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileDiff, FileStatus};

    fn cs(paths: &[&str]) -> Changeset {
        Changeset {
            files: paths
                .iter()
                .map(|p| FileDiff {
                    old_path: Some(p.into()),
                    new_path: Some(p.into()),
                    status: FileStatus::Modified,
                    binary: false,
                    too_large: false,
                    hunks: vec![],
                    old_content: None,
                    new_content: None,
                })
                .collect(),
        }
    }

    fn labels(t: &FileTree) -> Vec<String> {
        t.visible_items().map(|(i, _)| format!("{}{}", " ".repeat(i.depth), i.label)).collect()
    }

    #[test]
    fn builds_tree_with_merged_chains() {
        let t = FileTree::new(&cs(&["src/ui/a.rs", "src/ui/b.rs", "README.md", "src/main.rs"]));
        assert_eq!(labels(&t), vec!["src", " ui", "  a.rs", "  b.rs", " main.rs", "README.md"]);

        let t = FileTree::new(&cs(&["a/b/c/x.rs"]));
        assert_eq!(labels(&t), vec!["a/b/c", " x.rs"]);
    }

    #[test]
    fn collapse_expand_and_parent() {
        let mut t = FileTree::new(&cs(&["src/ui/a.rs", "src/main.rs", "README.md"]));
        t.selected = 1; // ui
        assert!(t.set_expanded(Some(false)));
        assert_eq!(labels(&t), vec!["src", " ui", " main.rs", "README.md"]);
        t.select_file(0); // a.rs is hidden: selection unchanged
        assert_eq!(t.selected, 1);
        t.set_expanded(None);
        t.select_file(0);
        assert_eq!(t.selected, 2);
        t.select_parent();
        assert_eq!(t.selected, 1);
    }
}
