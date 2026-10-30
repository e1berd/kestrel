use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kestrel_core::Item;

#[cfg(test)]
mod tests;

pub(crate) struct TreeNode {
    pub path: Arc<Path>,
    pub name: String,
    pub method: Option<String>,
    pub depth: usize,
    pub count: usize,
    parent: Option<usize>,
    end: usize,
    search: String,
}

pub(crate) struct TreeIndex {
    pub nodes: Vec<TreeNode>,
    paths: HashMap<Arc<Path>, usize>,
}

impl TreeIndex {
    pub fn new(items: &[Item]) -> Self {
        let mut index = Self { nodes: Vec::new(), paths: HashMap::new() };
        let mut stack = vec![(items.iter(), None, 0)];
        while let Some((items, parent, depth)) = stack.last_mut() {
            if let Some(item) = items.next() {
                let node = index.nodes.len();
                let name = item.name();
                let (method, url) = match item {
                    Item::Folder(_) => (None, ""),
                    Item::Request(request) => (Some(request.method()), request.doc.data.http.as_ref().map_or("", |http| &http.url)),
                };
                let path: Arc<Path> = item.path().into();
                index.paths.insert(path.clone(), node);
                index.nodes.push(TreeNode {
                    path,
                    search: format!("{}\n{}", name.to_lowercase(), url.to_lowercase()),
                    name,
                    count: usize::from(method.is_some()),
                    method,
                    parent: *parent,
                    depth: *depth,
                    end: node + 1,
                });
                if let Item::Folder(folder) = item {
                    let next_depth = *depth + 1;
                    stack.push((folder.children.iter(), Some(node), next_depth));
                }
            } else {
                if let Some(parent) = *parent {
                    index.nodes[parent].end = index.nodes.len();
                }
                stack.pop();
            }
        }
        for node in (0..index.nodes.len()).rev() {
            if let Some(parent) = index.nodes[node].parent {
                index.nodes[parent].count += index.nodes[node].count;
            }
        }
        index
    }

    pub fn matching_rows(&self, query: &str) -> Vec<usize> {
        let query = query.to_lowercase();
        let mut matches: Vec<_> = self.nodes.iter().map(|node| node.search.contains(&query)).collect();
        for node in (0..self.nodes.len()).rev() {
            if matches[node]
                && let Some(parent) = self.nodes[node].parent
            {
                matches[parent] = true;
            }
        }
        matches.into_iter().enumerate().filter_map(|(node, matches)| matches.then_some(node)).collect()
    }

    fn expanded_rows(&self, start: usize, end: usize, expanded: &HashSet<PathBuf>) -> Vec<usize> {
        let mut rows = Vec::new();
        let mut node = start;
        while node < end {
            rows.push(node);
            let item = &self.nodes[node];
            node = if item.method.is_none() && !expanded.contains(item.path.as_ref()) { item.end } else { node + 1 };
        }
        rows
    }
}

pub(crate) struct SidebarTree {
    pub index: Arc<TreeIndex>,
    pub rows: Vec<usize>,
    pub filtered: bool,
}

impl SidebarTree {
    pub fn new(index: Arc<TreeIndex>, expanded: &HashSet<PathBuf>) -> Self {
        let rows = index.expanded_rows(0, index.nodes.len(), expanded);
        Self { index, rows, filtered: false }
    }

    pub fn show_expanded(&mut self, expanded: &HashSet<PathBuf>) {
        self.rows = self.index.expanded_rows(0, self.index.nodes.len(), expanded);
        self.filtered = false;
    }

    pub fn show_matches(&mut self, rows: Vec<usize>) {
        self.rows = rows;
        self.filtered = true;
    }

    pub fn toggle(&mut self, path: &Path, expanded: &mut HashSet<PathBuf>) {
        let opening = if expanded.remove(path) {
            false
        } else {
            expanded.insert(path.to_path_buf());
            true
        };
        if self.filtered {
            return;
        }
        let Some(&node) = self.index.paths.get(path) else { return };
        let Ok(position) = self.rows.binary_search(&node) else { return };
        let end = self.index.nodes[node].end;
        if opening {
            let children = self.index.expanded_rows(node + 1, end, expanded);
            self.rows.splice(position + 1..position + 1, children);
        } else {
            let last = self.rows.partition_point(|&row| row < end);
            self.rows.drain(position + 1..last);
        }
    }

    pub fn node(&self, row: usize) -> Option<&TreeNode> {
        self.index.nodes.get(*self.rows.get(row)?)
    }

    pub fn row_position(&self, path: &Path) -> Option<usize> {
        let node = self.index.paths.get(path)?;
        self.rows.binary_search(node).ok()
    }
}
