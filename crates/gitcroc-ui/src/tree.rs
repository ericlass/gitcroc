//! One workspace's tree as the core reports it, and what the user did with
//! it: expanded nodes, the selected row or repo rows (UI-6) and the filter
//! text (ARCH-9, UI-29). A port of rc.1's `stores/tree.ts` and
//! `tree/rows.ts`. The core builds, folds and filters the tree (ARCH-6);
//! this keeps its answers and lays them out as rows.

use std::collections::{HashMap, HashSet};

use gitcroc_core::{GroupId, GroupSummary, RepoId, SummaryView, TargetSet, TreeItem, TreeNode};

use crate::strings;

/// The repo an item belongs to; `None` for Ungrouped and a group's nodes.
pub fn repo_of(item: &TreeItem) -> Option<&RepoId> {
    match item {
        TreeItem::Repo { repo_id, .. }
        | TreeItem::Local { repo_id, .. }
        | TreeItem::Remotes { repo_id, .. }
        | TreeItem::Tags { repo_id, .. }
        | TreeItem::Stashes { repo_id, .. }
        | TreeItem::Remote { repo_id, .. }
        | TreeItem::Folder { repo_id }
        | TreeItem::Branch { repo_id, .. }
        | TreeItem::RemoteBranch { repo_id, .. }
        | TreeItem::Tag { repo_id, .. }
        | TreeItem::Stash { repo_id, .. } => Some(repo_id),
        TreeItem::Group { .. }
        | TreeItem::GroupBranches { .. }
        | TreeItem::GroupFolder { .. }
        | TreeItem::GroupBranch { .. }
        | TreeItem::Ungrouped => None,
    }
}

/// Where the loaded children of a node not loaded are kept: a repo's four
/// ref nodes for the place the repo appears in (UI-2), or a group's
/// Branches node by its group. `None` for nodes that are never loaded.
pub fn load_key(item: &TreeItem) -> Option<String> {
    match item {
        TreeItem::Local { repo_id, group_id }
        | TreeItem::Remotes { repo_id, group_id }
        | TreeItem::Tags { repo_id, group_id }
        | TreeItem::Stashes { repo_id, group_id } => Some(format!(
            "refs:{}:{}",
            group_id.as_ref().map_or("", |group| group.0.as_str()),
            repo_id.0
        )),
        TreeItem::GroupBranches { group_id } => Some(format!("branches:{}", group_id.0)),
        _ => None,
    }
}

/// One row on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The node's id; a message row's is its node's with `:message`.
    pub id: String,
    pub depth: usize,
    /// The id of the parent row's node; `None` at the top.
    pub parent: Option<String>,
    pub kind: RowKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    Node {
        label: String,
        item: TreeItem,
        expandable: bool,
        expanded: bool,
    },
    /// "Loading…" below a node not loaded yet, or why loading it failed.
    Message { text: String, error: bool },
}

impl Row {
    pub fn item(&self) -> Option<&TreeItem> {
        match &self.kind {
            RowKind::Node { item, .. } => Some(item),
            RowKind::Message { .. } => None,
        }
    }

    pub fn is_repo(&self) -> bool {
        matches!(self.item(), Some(TreeItem::Repo { .. }))
    }
}

/// A selected row.
#[derive(Debug, Clone, PartialEq)]
pub struct Selected {
    pub row: String,
    pub item: TreeItem,
}

impl Selected {
    fn of(row: &Row) -> Option<Self> {
        row.item().map(|item| Self {
            row: row.id.clone(),
            item: item.clone(),
        })
    }

    fn repo(&self) -> Option<&RepoId> {
        match &self.item {
            TreeItem::Repo { repo_id, .. } => Some(repo_id),
            _ => None,
        }
    }
}

/// One workspace's tree.
#[derive(Debug, Default)]
pub struct Tree {
    /// The skeleton from `tree_get`; `None` until the first read.
    roots: Option<Vec<TreeNode>>,
    /// Why the last `tree_get` failed.
    error: Option<String>,
    /// Loaded children by [`load_key`].
    loaded: HashMap<String, Vec<TreeNode>>,
    /// What each entry of `loaded` was loaded for, to load it again.
    loaded_for: HashMap<String, TreeItem>,
    load_errors: HashMap<String, String>,
    loading: HashSet<String>,
    /// Expanded or collapsed nodes by id; the others use their default.
    expanded: HashMap<String, bool>,
    /// The same while a filter is shown; cleared when the text changes.
    filter_expanded: HashMap<String, bool>,
    /// The selected row; while repo rows are multi-selected, the row the
    /// keyboard is on.
    selected: Option<Selected>,
    /// The multi-selected repo rows (UI-6), each repo once; `None` for a
    /// single selection.
    multi: Option<Vec<Selected>>,
    /// Where a Shift range starts.
    anchor: Option<String>,
    filter: String,
    /// The core's answer to `filter`; `None` while the filter is empty.
    filtered: Option<Vec<TreeNode>>,
    /// Each repo's summary (UI-3, PERF-3).
    summaries: HashMap<RepoId, SummaryView>,
    /// Group rows the core sent since the last tree read (UI-4), newer
    /// than the summaries in the tree's group nodes.
    groups: HashMap<GroupId, GroupSummary>,
}

impl Tree {
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Read, unfiltered, and without repos.
    pub fn is_empty(&self) -> bool {
        self.filtered.is_none()
            && self.roots.as_ref().is_some_and(|roots| {
                roots
                    .iter()
                    .all(|root| root.children.as_ref().is_some_and(Vec::is_empty))
            })
    }

    pub fn set_roots(&mut self, roots: Result<Vec<TreeNode>, String>) {
        match roots {
            Ok(roots) => {
                self.roots = Some(roots);
                self.error = None;
                self.groups.clear();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn set_tree_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Starts showing `text`'s matches: `true` when the core is to be asked.
    pub fn set_filter(&mut self, text: String) -> bool {
        self.filter_expanded.clear();
        if text.is_empty() {
            self.filtered = None;
        }
        self.filter = text;
        !self.filter.is_empty()
    }

    /// The core's answer for `text`; dropped when the text changed since.
    pub fn set_filtered(&mut self, text: &str, nodes: Vec<TreeNode>) {
        if self.filter == text && !text.is_empty() {
            self.filtered = Some(nodes);
        }
    }

    pub fn summary(&self, repo: &RepoId) -> Option<&SummaryView> {
        self.summaries.get(repo)
    }

    pub fn set_summaries(&mut self, views: Vec<SummaryView>) {
        self.summaries = views
            .into_iter()
            .map(|view| (view.repo_id.clone(), view))
            .collect();
    }

    pub fn set_summary(&mut self, view: SummaryView) {
        self.summaries.insert(view.repo_id.clone(), view);
    }

    pub fn set_group(&mut self, summary: GroupSummary) {
        self.groups.insert(summary.group_id.clone(), summary);
    }

    /// What a group node with `summary` shows now.
    pub fn group_summary<'a>(&'a self, summary: &'a GroupSummary) -> &'a GroupSummary {
        self.groups.get(&summary.group_id).unwrap_or(summary)
    }

    /// Expands or collapses node `id`. Returns the item to load when
    /// expanding needs children not loaded or being loaded.
    pub fn set_expanded(&mut self, row: &Row, expanded: bool) -> Option<TreeItem> {
        let toggled = if self.filtered.is_none() {
            &mut self.expanded
        } else {
            &mut self.filter_expanded
        };
        toggled.insert(row.id.clone(), expanded);
        let item = row.item()?;
        let key = load_key(item)?;
        (expanded && !self.loaded.contains_key(&key) && !self.loading.contains(&key))
            .then(|| item.clone())
    }

    pub fn start_load(&mut self, item: &TreeItem) {
        if let Some(key) = load_key(item) {
            self.loading.insert(key);
        }
    }

    pub fn finish_load(&mut self, item: TreeItem, nodes: Result<Vec<TreeNode>, String>) {
        let Some(key) = load_key(&item) else {
            return;
        };
        self.loading.remove(&key);
        match nodes {
            Ok(nodes) => {
                self.loaded.insert(key.clone(), nodes);
                self.loaded_for.insert(key.clone(), item);
                self.load_errors.remove(&key);
            }
            Err(error) => {
                self.load_errors.insert(key, error);
            }
        }
    }

    /// What is loaded and goes stale when `repo`'s refs change: its places,
    /// and the groups' Branches nodes, which may count it.
    pub fn stale_loads(&self, repo: &RepoId) -> Vec<TreeItem> {
        self.loaded_for
            .values()
            .filter(|item| {
                matches!(item, TreeItem::GroupBranches { .. }) || repo_of(item) == Some(repo)
            })
            .cloned()
            .collect()
    }

    /// Everything loaded, to load again after the app's events were missed.
    pub fn all_loads(&self) -> Vec<TreeItem> {
        self.loaded_for.values().cloned().collect()
    }

    /// `node`'s children: its own, or those loaded for it; `None` when not
    /// loaded.
    fn children_of<'a>(&'a self, node: &'a TreeNode) -> Option<&'a [TreeNode]> {
        if let Some(children) = &node.children {
            return Some(children);
        }
        let loaded = self.loaded.get(&load_key(&node.item)?)?;
        // A group's Branches node gets its children; a repo's four ref
        // nodes come together, each in place of the node of the same id.
        if matches!(node.item, TreeItem::GroupBranches { .. }) {
            return Some(loaded);
        }
        loaded
            .iter()
            .find(|loaded| loaded.id == node.id)?
            .children
            .as_deref()
    }

    /// The rows on screen: depth first, children only below expanded nodes.
    /// Ungrouped and the groups start expanded, everything else collapsed.
    /// While a filter is shown, every node the core sent with children
    /// starts expanded, so the matches are in view.
    pub fn rows(&self) -> Vec<Row> {
        let filtering = self.filtered.is_some();
        let roots = self
            .filtered
            .as_deref()
            .or(self.roots.as_deref())
            .unwrap_or_default();
        let mut rows = Vec::new();
        self.visit(roots, 0, None, filtering, &mut rows);
        rows
    }

    fn visit(
        &self,
        nodes: &[TreeNode],
        depth: usize,
        parent: Option<&str>,
        filtering: bool,
        rows: &mut Vec<Row>,
    ) {
        let toggled = if filtering {
            &self.filter_expanded
        } else {
            &self.expanded
        };
        for node in nodes {
            let children = self.children_of(node);
            let expandable = children.is_none_or(|children| !children.is_empty());
            let opened = if filtering && node.children.is_some() {
                true
            } else {
                matches!(node.item, TreeItem::Ungrouped | TreeItem::Group { .. })
            };
            let expanded = expandable && toggled.get(&node.id).copied().unwrap_or(opened);
            rows.push(Row {
                id: node.id.clone(),
                depth,
                parent: parent.map(str::to_owned),
                kind: RowKind::Node {
                    label: node.label.clone(),
                    item: node.item.clone(),
                    expandable,
                    expanded,
                },
            });
            if !expanded {
                continue;
            }
            if let Some(children) = children {
                self.visit(children, depth + 1, Some(&node.id), filtering, rows);
                continue;
            }
            let error = load_key(&node.item).and_then(|key| self.load_errors.get(&key));
            rows.push(Row {
                id: format!("{}:message", node.id),
                depth: depth + 1,
                parent: Some(node.id.clone()),
                kind: RowKind::Message {
                    text: error.map_or_else(|| strings::tree::LOADING.to_owned(), Clone::clone),
                    error: error.is_some(),
                },
            });
        }
    }

    /// The row the keyboard is on.
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_ref().map(|selected| selected.row.as_str())
    }

    /// The item of the row the keyboard is on.
    pub fn selected_item(&self) -> Option<&TreeItem> {
        self.selected.as_ref().map(|selected| &selected.item)
    }

    /// Whether row `id` is selected.
    pub fn is_selected(&self, id: &str) -> bool {
        match &self.multi {
            Some(multi) => multi.iter().any(|selected| selected.row == id),
            None => self.selected() == Some(id),
        }
    }

    /// The repo of the row the keyboard is on, if any.
    pub fn selected_repo(&self) -> Option<&RepoId> {
        self.selected
            .as_ref()
            .and_then(|selected| repo_of(&selected.item))
    }

    /// Puts the keyboard on `row`, with `multi` selected. Returns the row's
    /// repo, for the core, when the keyboard moved to another row.
    fn place(
        &mut self,
        row: Option<Selected>,
        multi: Option<Vec<Selected>>,
        anchor: Option<String>,
    ) -> Option<Option<RepoId>> {
        let moved = self.selected.as_ref().map(|selected| &selected.row)
            != row.as_ref().map(|row| &row.row);
        self.selected = row;
        self.multi = multi;
        self.anchor = anchor;
        moved.then(|| self.selected_repo().cloned())
    }

    /// Selects `row` alone, replacing a multi-selection.
    pub fn select(&mut self, row: &Row) -> Option<Option<RepoId>> {
        let selected = Selected::of(row)?;
        if self.selected() == Some(row.id.as_str()) && self.multi.is_none() {
            return None;
        }
        self.place(Some(selected), None, Some(row.id.clone()))
    }

    /// The selected repo rows: the multi-selection, or the single selection
    /// when it is a repo row.
    fn picked(&self) -> Vec<Selected> {
        match (&self.multi, &self.selected) {
            (Some(multi), _) => multi.clone(),
            (None, Some(selected)) if selected.repo().is_some() => vec![selected.clone()],
            (None, _) => Vec::new(),
        }
    }

    /// Ctrl/Cmd-click or Ctrl/Cmd+Space (UI-6): a repo row joins the
    /// selection, or leaves it with every row of its repo, which counts
    /// once. Any other row is selected alone.
    pub fn toggle(&mut self, row: &Row) -> Option<Option<RepoId>> {
        let picked = Selected::of(row).filter(|picked| picked.repo().is_some());
        let Some(picked) = picked else {
            return self.select(row);
        };
        let mut rows = self.picked();
        let known = rows.iter().any(|row| row.repo() == picked.repo());
        if known {
            rows.retain(|row| row.repo() != picked.repo());
        } else {
            rows.push(picked.clone());
        }
        self.place(Some(picked), Some(rows), Some(row.id.clone()))
    }

    /// Shift-click or Shift+Up/Down (UI-6): the repo rows of `rows` from
    /// the anchor to `row`, each repo once. Any other row is selected alone.
    pub fn extend(&mut self, rows: &[Row], row: &Row) -> Option<Option<RepoId>> {
        if !row.is_repo() {
            return self.select(row);
        }
        let anchor = self
            .anchor
            .clone()
            .or_else(|| self.selected().map(str::to_owned))
            .unwrap_or_else(|| row.id.clone());
        let from = rows.iter().position(|candidate| candidate.id == anchor);
        let to = rows.iter().position(|candidate| candidate.id == row.id);
        let (Some(from), Some(to)) = (from, to) else {
            // The anchor is no longer shown: the row is selected alone.
            return self.place(Selected::of(row), None, Some(row.id.clone()));
        };
        let mut multi: Vec<Selected> = Vec::new();
        for candidate in &rows[from.min(to)..=from.max(to)] {
            if let Some(picked) = Selected::of(candidate).filter(|picked| picked.repo().is_some())
                && !multi.iter().any(|known| known.repo() == picked.repo())
            {
                multi.push(picked);
            }
        }
        self.place(Selected::of(row), Some(multi), Some(anchor))
    }

    /// Moves the keyboard to `row`, keeping the selected repo rows
    /// (Ctrl/Cmd+Up/Down).
    pub fn move_cursor(&mut self, row: &Row) -> Option<Option<RepoId>> {
        let picked = self.picked();
        let anchor = self.anchor.clone();
        self.place(Selected::of(row), Some(picked), anchor)
    }

    /// Selects the row of `repo` within the selection, alone (UI-11): its
    /// row under the selected group, or its multi-selected row. Returns the
    /// row's repo, for the core, when the keyboard moved to it; nothing
    /// changes when the selection has no such row.
    pub fn select_member(&mut self, repo: &RepoId) -> Option<Option<RepoId>> {
        let member = match &self.multi {
            Some(multi) => multi
                .iter()
                .find(|selected| selected.repo() == Some(repo))
                .cloned(),
            None => {
                let group = self.selected.as_ref()?;
                if !matches!(group.item, TreeItem::Group { .. }) {
                    return None;
                }
                self.roots
                    .iter()
                    .flatten()
                    .find(|node| node.id == group.row)
                    .and_then(|node| node.children.as_ref())
                    .into_iter()
                    .flatten()
                    .find(|node| {
                        matches!(&node.item, TreeItem::Repo { repo_id, .. } if repo_id == repo)
                    })
                    .map(|node| Selected {
                        row: node.id.clone(),
                        item: node.item.clone(),
                    })
            }
        }?;
        let anchor = member.row.clone();
        self.place(Some(member), None, Some(anchor))
    }

    /// Each repo's name, from its rows (UI-1).
    pub fn repo_names(&self) -> HashMap<RepoId, String> {
        let mut names = HashMap::new();
        // Repos are at the top or one level down, under a group or
        // Ungrouped; their own children are refs.
        for node in self.roots.iter().flatten() {
            for node in std::iter::once(node).chain(node.children.iter().flatten()) {
                if let TreeItem::Repo { repo_id, .. } = &node.item {
                    names
                        .entry(repo_id.clone())
                        .or_insert_with(|| node.label.clone());
                }
            }
        }
        names
    }

    /// What the toolbar acts on (GRP-5): the selected row's repo, the
    /// selected group, or the multi-selected repos; `None` for anything
    /// else.
    pub fn target(&self) -> Option<TargetSet> {
        let single = match &self.multi {
            Some(multi) if multi.len() > 1 => {
                return Some(TargetSet::Repos {
                    repo_ids: multi.iter().filter_map(Selected::repo).cloned().collect(),
                });
            }
            Some(multi) => multi.first(),
            None => self.selected.as_ref(),
        }?;
        match &single.item {
            TreeItem::Group { group_id, .. } => Some(TargetSet::Group {
                group_id: group_id.clone(),
            }),
            item => repo_of(item).map(|repo| TargetSet::Repo {
                repo_id: repo.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(id: &str, group: Option<&str>) -> TreeNode {
        let place = group.map_or(String::new(), |group| format!("group:{group}:"));
        let repo_id = RepoId(id.to_owned());
        let group_id = group.map(|group| GroupId(group.to_owned()));
        TreeNode {
            id: format!("{place}repo:{id}"),
            label: id.to_owned(),
            item: TreeItem::Repo {
                repo_id: repo_id.clone(),
                group_id: group_id.clone(),
            },
            children: Some(vec![TreeNode {
                id: format!("{place}repo:{id}:local"),
                label: "Local".to_owned(),
                item: TreeItem::Local { repo_id, group_id },
                children: None,
            }]),
        }
    }

    fn ungrouped(repos: Vec<TreeNode>) -> TreeNode {
        TreeNode {
            id: "ungrouped".to_owned(),
            label: "Ungrouped".to_owned(),
            item: TreeItem::Ungrouped,
            children: Some(repos),
        }
    }

    fn ids(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|row| row.id.as_str()).collect()
    }

    fn row<'a>(rows: &'a [Row], id: &str) -> &'a Row {
        rows.iter().find(|row| row.id == id).unwrap()
    }

    #[test]
    fn ungrouped_starts_open_and_a_node_not_loaded_says_so() {
        let mut tree = Tree::default();
        tree.set_roots(Ok(vec![ungrouped(vec![repo("a", None)])]));
        let rows = tree.rows();
        assert_eq!(ids(&rows), ["ungrouped", "repo:a"]);

        let load = tree.set_expanded(row(&rows, "repo:a"), true);
        assert_eq!(load, None, "a repo's own children are there");
        let rows = tree.rows();
        let load = tree.set_expanded(row(&rows, "repo:a:local"), true);
        assert!(matches!(load, Some(TreeItem::Local { .. })));
        let rows = tree.rows();
        assert_eq!(
            rows.last().unwrap().kind,
            RowKind::Message {
                text: strings::tree::LOADING.to_owned(),
                error: false
            }
        );

        let local = TreeNode {
            id: "repo:a:local".to_owned(),
            label: "Local".to_owned(),
            item: TreeItem::Local {
                repo_id: RepoId("a".to_owned()),
                group_id: None,
            },
            children: Some(Vec::new()),
        };
        tree.finish_load(local.item.clone(), Ok(vec![local]));
        let rows = tree.rows();
        assert_eq!(ids(&rows), ["ungrouped", "repo:a", "repo:a:local"]);
        assert!(matches!(
            row(&rows, "repo:a:local").kind,
            RowKind::Node {
                expandable: false,
                ..
            }
        ));
    }

    #[test]
    fn a_multi_selection_holds_repo_rows_once_each() {
        let mut tree = Tree::default();
        tree.set_roots(Ok(vec![ungrouped(vec![
            repo("a", None),
            repo("b", None),
            repo("c", None),
        ])]));
        let rows = tree.rows();
        assert_eq!(
            tree.select(row(&rows, "repo:a")),
            Some(Some(RepoId("a".to_owned())))
        );
        tree.extend(&rows, row(&rows, "repo:c"));
        assert_eq!(
            tree.target(),
            Some(TargetSet::Repos {
                repo_ids: ["a", "b", "c"].map(|id| RepoId(id.to_owned())).to_vec()
            })
        );
        tree.toggle(row(&rows, "repo:b"));
        assert!(!tree.is_selected("repo:b"));
        assert!(tree.is_selected("repo:a") && tree.is_selected("repo:c"));

        // Ungrouped is selected alone.
        tree.toggle(row(&rows, "ungrouped"));
        assert!(!tree.is_selected("repo:a"));
        assert_eq!(tree.target(), None);
    }

    #[test]
    fn a_filter_answer_for_older_text_is_dropped() {
        let mut tree = Tree::default();
        tree.set_roots(Ok(vec![ungrouped(vec![repo("a", None)])]));
        assert!(tree.set_filter("a".to_owned()));
        assert!(tree.set_filter("ab".to_owned()));
        tree.set_filtered("a", Vec::new());
        assert_eq!(ids(&tree.rows()), ["ungrouped", "repo:a"]);
        tree.set_filtered("ab", Vec::new());
        assert!(tree.rows().is_empty());
        assert!(!tree.set_filter(String::new()));
        assert_eq!(ids(&tree.rows()), ["ungrouped", "repo:a"]);
    }
}
