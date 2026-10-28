//! What one workspace's changes pane shows (UI-14, UI-16, UI-34): the
//! history's selection, the core's view of it, and the rows of its file
//! lists, flat or as a folder tree, with the selected file. A port of rc.1's
//! `stores/changes.ts` and `changes/fileRows.ts`, without a UI toolkit and
//! without the diff (M6-13). The core reads the lists (ARCH-6) and names
//! each file by list and id, never by path (PLAT-2); this keeps its latest
//! answer and lays its lists out.

use std::collections::{BTreeMap, HashSet};

use gitcroc_core::{
    ChangedFile, ChangesContent, ChangesTarget, ChangesView, FileListMode, RepoId, TargetSet,
    UnmergedFile,
};

use crate::history::Selection;

/// The list a file is in. Unmerged files are a list of their own, always
/// by full path (OPS-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum List {
    Commit,
    Unmerged,
    Staged,
    Unstaged,
}

/// A file of the view: its list, its id there and, in a merged view, its
/// repo (UI-34).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileKey {
    pub list: List,
    pub id: usize,
    pub repo: Option<RepoId>,
}

/// One row of the file lists.
#[derive(Debug, Clone, PartialEq)]
pub enum FileRow {
    /// Unmerged, Staged or Unstaged, with its file count.
    Section { list: List, count: usize },
    /// A repo's part of a section in a merged view, under its name; why its
    /// files cannot be changed when it is blocked (GRP-8).
    Repo {
        repo: RepoId,
        name: String,
        list: List,
        count: usize,
        blocked: Option<String>,
    },
    /// A folder of a tree. `key` names it for collapsing.
    Folder {
        key: String,
        name: String,
        depth: usize,
        collapsed: bool,
    },
    /// A file: its full path in a flat list, its name in a tree.
    File {
        file: FileKey,
        name: String,
        depth: usize,
    },
}

/// A listed file, as the rows need it.
#[derive(Debug, Clone, Copy)]
pub struct Listed<'a> {
    pub id: usize,
    pub path: &'a str,
}

/// One repo's lists in a merged view, in target-set order.
#[derive(Debug, Clone)]
pub struct RepoLists<'a> {
    pub repo: &'a RepoId,
    pub name: &'a str,
    pub blocked: Option<&'a str>,
    pub staged: Vec<Listed<'a>>,
    pub unstaged: Vec<Listed<'a>>,
    pub unmerged: Vec<Listed<'a>>,
}

/// What the rows are made of: a view's lists, by path.
#[derive(Debug, Clone)]
pub enum Lists<'a> {
    Commit(Vec<Listed<'a>>),
    Uncommitted {
        staged: Vec<Listed<'a>>,
        unstaged: Vec<Listed<'a>>,
        unmerged: Vec<Listed<'a>>,
    },
    Merged(Vec<RepoLists<'a>>),
}

fn listed(files: &[ChangedFile]) -> Vec<Listed<'_>> {
    files
        .iter()
        .map(|file| Listed {
            id: file.id,
            path: &file.path,
        })
        .collect()
}

fn listed_unmerged(files: &[UnmergedFile]) -> Vec<Listed<'_>> {
    files
        .iter()
        .map(|file| Listed {
            id: file.id,
            path: &file.path,
        })
        .collect()
}

/// The lists of `content`.
pub fn lists(content: &ChangesContent) -> Lists<'_> {
    match content {
        ChangesContent::Commit { files, .. } => Lists::Commit(listed(files)),
        ChangesContent::Uncommitted {
            staged,
            unstaged,
            unmerged,
            ..
        } => Lists::Uncommitted {
            staged: listed(staged),
            unstaged: listed(unstaged),
            unmerged: listed_unmerged(unmerged),
        },
        ChangesContent::Merged { repos, .. } => Lists::Merged(
            repos
                .iter()
                .map(|repo| RepoLists {
                    repo: &repo.repo_id,
                    name: &repo.repo_name,
                    blocked: repo.blocked.as_deref(),
                    staged: listed(&repo.staged),
                    unstaged: listed(&repo.unstaged),
                    unmerged: listed_unmerged(&repo.unmerged),
                })
                .collect(),
        ),
    }
}

fn list_key(list: List, repo: Option<&RepoId>) -> String {
    let list = match list {
        List::Commit => "commit",
        List::Unmerged => "unmerged",
        List::Staged => "staged",
        List::Unstaged => "unstaged",
    };
    match repo {
        Some(repo) => format!("{}/{list}", repo.0),
        None => list.to_owned(),
    }
}

fn file_row(
    list: List,
    file: Listed,
    name: String,
    depth: usize,
    repo: Option<&RepoId>,
) -> FileRow {
    FileRow::File {
        file: FileKey {
            list,
            id: file.id,
            repo: repo.cloned(),
        },
        name,
        depth,
    }
}

#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<&'a str, Folder<'a>>,
    files: Vec<(String, Listed<'a>)>,
}

/// The files of `list` as a folder tree from depth `base`: folders first by
/// name, then files in list order. A folder whose only child is a folder
/// shows as one row, `src/util`.
fn tree_rows(
    list: List,
    files: &[Listed],
    collapsed: &HashSet<String>,
    base: usize,
    repo: Option<&RepoId>,
) -> Vec<FileRow> {
    let mut root = Folder::default();
    for file in files {
        // A folder git lists as one entry ends in `/`; it stays one row.
        let directory = file.path.ends_with('/');
        let path = file.path.strip_suffix('/').unwrap_or(file.path);
        let mut parts: Vec<&str> = path.split('/').collect();
        let last = parts.pop().unwrap_or(path);
        let name = if directory {
            format!("{last}/")
        } else {
            last.to_owned()
        };
        let mut folder = &mut root;
        for part in parts {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.push((name, *file));
    }
    let mut rows = Vec::new();
    let prefix = list_key(list, repo);
    walk(&root, "", base, list, &prefix, collapsed, repo, &mut rows);
    rows
}

#[allow(clippy::too_many_arguments)]
fn walk(
    folder: &Folder,
    path: &str,
    depth: usize,
    list: List,
    prefix: &str,
    collapsed: &HashSet<String>,
    repo: Option<&RepoId>,
    rows: &mut Vec<FileRow>,
) {
    for (name, child) in &folder.folders {
        let mut label = (*name).to_owned();
        let mut child = child;
        while child.files.is_empty() && child.folders.len() == 1 {
            let Some((inner, only)) = child.folders.iter().next() else {
                break;
            };
            label = format!("{label}/{inner}");
            child = only;
        }
        let full = format!("{path}{label}/");
        let key = format!("{prefix}:dir:{full}");
        let closed = collapsed.contains(&key);
        rows.push(FileRow::Folder {
            key,
            name: label,
            depth,
            collapsed: closed,
        });
        if !closed {
            walk(child, &full, depth + 1, list, prefix, collapsed, repo, rows);
        }
    }
    for (name, file) in &folder.files {
        rows.push(file_row(list, *file, name.clone(), depth, repo));
    }
}

fn list_rows(
    list: List,
    files: &[Listed],
    mode: FileListMode,
    collapsed: &HashSet<String>,
    base: usize,
    repo: Option<&RepoId>,
) -> Vec<FileRow> {
    match mode {
        // Unmerged files are always by full path (OPS-3).
        FileListMode::Tree if list != List::Unmerged => {
            tree_rows(list, files, collapsed, base, repo)
        }
        _ => files
            .iter()
            .map(|file| file_row(list, *file, file.path.to_owned(), base, repo))
            .collect(),
    }
}

/// The rows of `lists`. A commit has one list; uncommitted changes have a
/// Staged and an Unstaged section, and before them, while files are
/// unmerged, an Unmerged one. A merged view has the same sections with one
/// part per repo that has files there, in target-set order (UI-34).
pub fn file_rows(lists: &Lists, mode: FileListMode, collapsed: &HashSet<String>) -> Vec<FileRow> {
    match lists {
        Lists::Commit(files) => list_rows(List::Commit, files, mode, collapsed, 0, None),
        Lists::Uncommitted {
            staged,
            unstaged,
            unmerged,
        } => {
            let mut rows = Vec::new();
            for (list, files) in [
                (List::Unmerged, unmerged),
                (List::Staged, staged),
                (List::Unstaged, unstaged),
            ] {
                if list == List::Unmerged && files.is_empty() {
                    continue;
                }
                rows.push(FileRow::Section {
                    list,
                    count: files.len(),
                });
                rows.extend(list_rows(list, files, mode, collapsed, 0, None));
            }
            rows
        }
        Lists::Merged(repos) => {
            let mut rows = Vec::new();
            for list in [List::Unmerged, List::Staged, List::Unstaged] {
                let of = |repo: &'_ RepoLists<'_>| -> usize {
                    match list {
                        List::Unmerged => repo.unmerged.len(),
                        List::Staged => repo.staged.len(),
                        _ => repo.unstaged.len(),
                    }
                };
                let count: usize = repos.iter().map(of).sum();
                if list == List::Unmerged && count == 0 {
                    continue;
                }
                rows.push(FileRow::Section { list, count });
                for repo in repos.iter().filter(|repo| of(repo) > 0) {
                    let files = match list {
                        List::Unmerged => &repo.unmerged,
                        List::Staged => &repo.staged,
                        _ => &repo.unstaged,
                    };
                    rows.push(FileRow::Repo {
                        repo: repo.repo.clone(),
                        name: repo.name.to_owned(),
                        list,
                        count: files.len(),
                        blocked: repo.blocked.map(str::to_owned),
                    });
                    rows.extend(list_rows(list, files, mode, collapsed, 1, Some(repo.repo)));
                }
            }
            rows
        }
    }
}

/// A file of a view, as the list shows it.
#[derive(Debug, Clone, Copy)]
pub enum Entry<'a> {
    Changed(&'a ChangedFile),
    Unmerged(&'a UnmergedFile),
}

impl Entry<'_> {
    pub fn path(&self) -> &str {
        match self {
            Entry::Changed(file) => &file.path,
            Entry::Unmerged(file) => &file.path,
        }
    }
}

/// The entry with `id` in `files`; ids are list positions, so it is
/// usually at `id`.
fn by_id<T>(files: &[T], id: usize, id_of: impl Fn(&T) -> usize) -> Option<&T> {
    files
        .get(id)
        .filter(|file| id_of(file) == id)
        .or_else(|| files.iter().find(|file| id_of(file) == id))
}

/// The entry `file` names in `content`.
pub fn entry_of<'a>(content: &'a ChangesContent, file: &FileKey) -> Option<Entry<'a>> {
    let changed = |files: &'a [ChangedFile]| by_id(files, file.id, |f| f.id).map(Entry::Changed);
    let unmerged = |files: &'a [UnmergedFile]| by_id(files, file.id, |f| f.id).map(Entry::Unmerged);
    match (content, file.list) {
        (ChangesContent::Commit { files, .. }, List::Commit) => changed(files),
        (ChangesContent::Uncommitted { staged, .. }, List::Staged) => changed(staged),
        (ChangesContent::Uncommitted { unstaged, .. }, List::Unstaged) => changed(unstaged),
        (
            ChangesContent::Uncommitted {
                unmerged: files, ..
            },
            List::Unmerged,
        ) => unmerged(files),
        (ChangesContent::Merged { repos, .. }, list) => {
            let repo = repos
                .iter()
                .find(|repo| Some(&repo.repo_id) == file.repo.as_ref())?;
            match list {
                List::Staged => changed(&repo.staged),
                List::Unstaged => changed(&repo.unstaged),
                List::Unmerged => unmerged(&repo.unmerged),
                List::Commit => None,
            }
        }
        _ => None,
    }
}

/// A file to select again when its view is read anew: by list and path,
/// and in a merged view repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub list: List,
    pub path: String,
    pub repo: Option<RepoId>,
}

/// What to read for a selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    Repo { repo: RepoId, target: ChangesTarget },
    Merged(TargetSet),
}

/// One workspace's changes pane.
#[derive(Debug, Default)]
pub struct Changes {
    selection: Option<Selection>,
    /// The parent a commit is compared with (UI-16).
    parent: u32,
    view: Option<ChangesView>,
    loading: bool,
    error: Option<String>,
    file: Option<FileKey>,
    /// Closed folders of the tree, by row key; kept across selections.
    collapsed: HashSet<String>,
    mode: FileListMode,
    rows: Vec<FileRow>,
    /// While a view is read, the file to select again in it.
    keep: Option<Kept>,
}

impl Changes {
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn parent(&self) -> u32 {
        self.parent
    }

    pub fn view(&self) -> Option<&ChangesView> {
        self.view.as_ref()
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn file(&self) -> Option<&FileKey> {
        self.file.as_ref()
    }

    pub fn rows(&self) -> &[FileRow] {
        &self.rows
    }

    pub fn mode(&self) -> FileListMode {
        self.mode
    }

    /// The entry of the selected file.
    pub fn entry(&self) -> Option<Entry<'_>> {
        entry_of(&self.view.as_ref()?.content, self.file.as_ref()?)
    }

    /// The entry of `file`.
    pub fn entry_of(&self, file: &FileKey) -> Option<Entry<'_>> {
        entry_of(&self.view.as_ref()?.content, file)
    }

    /// The number of files the view lists.
    pub fn file_count(&self) -> usize {
        match self.view.as_ref().map(|view| &view.content) {
            None => 0,
            Some(ChangesContent::Commit { files, .. }) => files.len(),
            Some(ChangesContent::Uncommitted {
                staged,
                unstaged,
                unmerged,
                ..
            }) => staged.len() + unstaged.len() + unmerged.len(),
            Some(ChangesContent::Merged { repos, .. }) => repos
                .iter()
                .map(|repo| repo.staged.len() + repo.unstaged.len() + repo.unmerged.len())
                .sum(),
        }
    }

    /// Shows `selection`. The same selection again changes nothing; another
    /// one starts from its first parent, with nothing read yet.
    pub fn show(&mut self, selection: Option<Selection>) -> bool {
        if self.selection == selection {
            return false;
        }
        self.selection = selection;
        self.parent = 0;
        self.view = None;
        self.loading = false;
        self.error = None;
        self.file = None;
        self.rows.clear();
        self.keep = None;
        true
    }

    /// What to read for the shown selection.
    pub fn read(&self) -> Option<Read> {
        Some(match self.selection.as_ref()? {
            Selection::Commit { repo, hash } => Read::Repo {
                repo: repo.clone(),
                target: ChangesTarget::Commit {
                    hash: hash.clone(),
                    parent: self.parent,
                },
            },
            Selection::Uncommitted { repo } => Read::Repo {
                repo: repo.clone(),
                target: ChangesTarget::Uncommitted,
            },
            Selection::MergedUncommitted { target } => Read::Merged(target.clone()),
        })
    }

    /// Compares a merge commit with another parent (UI-16); its files are
    /// read again.
    pub fn set_parent(&mut self, parent: u32) -> bool {
        if self.parent == parent || !matches!(self.selection, Some(Selection::Commit { .. })) {
            return false;
        }
        self.parent = parent;
        true
    }

    /// The selected file, to select again when the view is read anew, or
    /// the one a read under way is to select again.
    pub fn kept(&self) -> Option<Kept> {
        let kept = self.file.as_ref().and_then(|file| {
            Some(Kept {
                list: file.list,
                path: self.entry_of(file)?.path().to_owned(),
                repo: file.repo.clone(),
            })
        });
        kept.or_else(|| self.keep.clone())
    }

    /// A read of the view starts; `keep` is the file to select again.
    pub fn start_load(&mut self, keep: Option<Kept>) {
        self.loading = true;
        self.error = None;
        self.keep = keep;
    }

    /// The view arrived. The kept file is selected again: in the same list,
    /// else in the other one, as after staging it whole; otherwise the
    /// first file (UI-14).
    pub fn set_view(&mut self, view: ChangesView) {
        self.loading = false;
        self.error = None;
        let keep = self.keep.take();
        self.view = Some(view);
        self.rebuild();
        let again = keep.and_then(|keep| self.find(&keep));
        self.file = again.or_else(|| self.first_file());
    }

    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.view = None;
        self.file = None;
        self.rows.clear();
        self.error = Some(error);
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// Lists the files flat or as a tree (UI-14).
    pub fn set_mode(&mut self, mode: FileListMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.rebuild();
        true
    }

    pub fn toggle_folder(&mut self, key: &str) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.to_owned());
        }
        self.rebuild();
    }

    pub fn select(&mut self, file: FileKey) {
        self.file = Some(file);
    }

    /// The row of the selected file.
    pub fn selected_index(&self) -> Option<usize> {
        let file = self.file.as_ref()?;
        self.rows
            .iter()
            .position(|row| matches!(row, FileRow::File { file: shown, .. } if shown == file))
    }

    /// The file row after (`step` 1) or before (-1) the selected one, or
    /// the first file when none is selected.
    pub fn step(&self, step: isize) -> Option<usize> {
        let files = self.file_indices();
        let at = self
            .selected_index()
            .and_then(|index| files.iter().position(|file| *file == index));
        let next = match at {
            Some(at) => at.checked_add_signed(step)?,
            None => 0,
        };
        files.get(next).copied()
    }

    /// The first (`last` false) or last file row.
    pub fn end(&self, last: bool) -> Option<usize> {
        let files = self.file_indices();
        if last {
            files.last().copied()
        } else {
            files.first().copied()
        }
    }

    fn file_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, FileRow::File { .. }))
            .map(|(index, _)| index)
            .collect()
    }

    /// Selects the file of row `index`; `false` when it is no file.
    pub fn select_index(&mut self, index: usize) -> bool {
        match self.rows.get(index) {
            Some(FileRow::File { file, .. }) => {
                self.file = Some(file.clone());
                true
            }
            _ => false,
        }
    }

    /// The repos whose working copies are shown: the repo of uncommitted
    /// changes, or every member of a merged view's target set.
    pub fn watched(&self) -> Vec<RepoId> {
        match (
            &self.selection,
            self.view.as_ref().map(|view| &view.content),
        ) {
            (Some(Selection::Uncommitted { repo }), _) => vec![repo.clone()],
            (_, Some(ChangesContent::Merged { members, .. })) => members.clone(),
            _ => Vec::new(),
        }
    }

    fn rebuild(&mut self) {
        self.rows = match &self.view {
            Some(view) => file_rows(&lists(&view.content), self.mode, &self.collapsed),
            None => Vec::new(),
        };
    }

    /// The first file the lists show, collapsed folders aside.
    fn first_file(&self) -> Option<FileKey> {
        let view = self.view.as_ref()?;
        file_rows(&lists(&view.content), self.mode, &HashSet::new())
            .into_iter()
            .find_map(|row| match row {
                FileRow::File { file, .. } => Some(file),
                _ => None,
            })
    }

    /// The file `keep` names: its path in its list, else in another list
    /// of its repo.
    fn find(&self, keep: &Kept) -> Option<FileKey> {
        let view = self.view.as_ref()?;
        let files = file_keys(&view.content);
        let with_path = |file: &&FileKey| {
            file.repo == keep.repo
                && entry_of(&view.content, file).is_some_and(|entry| entry.path() == keep.path)
        };
        files
            .iter()
            .filter(|file| file.list == keep.list)
            .find(with_path)
            .or_else(|| files.iter().find(with_path))
            .cloned()
    }
}

/// Every file of `content`, unmerged ones included.
fn file_keys(content: &ChangesContent) -> Vec<FileKey> {
    let keys = |list: List, ids: Vec<usize>, repo: Option<&RepoId>| {
        let repo = repo.cloned();
        ids.into_iter().map(move |id| FileKey {
            list,
            id,
            repo: repo.clone(),
        })
    };
    let ids = |files: &[ChangedFile]| files.iter().map(|file| file.id).collect::<Vec<_>>();
    let unmerged_ids =
        |files: &[UnmergedFile]| files.iter().map(|file| file.id).collect::<Vec<_>>();
    match content {
        ChangesContent::Commit { files, .. } => keys(List::Commit, ids(files), None).collect(),
        ChangesContent::Uncommitted {
            staged,
            unstaged,
            unmerged,
            ..
        } => keys(List::Staged, ids(staged), None)
            .chain(keys(List::Unmerged, unmerged_ids(unmerged), None))
            .chain(keys(List::Unstaged, ids(unstaged), None))
            .collect(),
        ChangesContent::Merged { repos, .. } => repos
            .iter()
            .flat_map(|repo| {
                let id = Some(&repo.repo_id);
                keys(List::Staged, ids(&repo.staged), id)
                    .chain(keys(List::Unmerged, unmerged_ids(&repo.unmerged), id))
                    .chain(keys(List::Unstaged, ids(&repo.unstaged), id))
                    .collect::<Vec<_>>()
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files<'a>(paths: &[&'a str]) -> Vec<Listed<'a>> {
        paths
            .iter()
            .enumerate()
            .map(|(id, path)| Listed { id, path })
            .collect()
    }

    /// The rows as `depth name` lines; folders end in `/`, closed ones in `+`.
    fn shown(rows: &[FileRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                FileRow::Section { list, count } => format!("[{list:?} {count}]"),
                FileRow::Repo { name, count, .. } => format!("<{name} {count}>"),
                FileRow::Folder {
                    name,
                    depth,
                    collapsed,
                    ..
                } => format!("{depth} {name}/{}", if *collapsed { "+" } else { "" }),
                FileRow::File { name, depth, .. } => format!("{depth} {name}"),
            })
            .collect()
    }

    #[test]
    fn ui_14_flat_rows_are_full_paths_in_list_order() {
        let lists = Lists::Commit(files(&["src/b.rs", "README.md", "src/a.rs"]));
        let rows = file_rows(&lists, FileListMode::Flat, &HashSet::new());
        assert_eq!(shown(&rows), ["0 src/b.rs", "0 README.md", "0 src/a.rs"]);
    }

    #[test]
    fn ui_14_tree_rows_put_folders_first_and_join_single_child_folders() {
        let lists = Lists::Commit(files(&[
            "src/util/deep/x.rs",
            "README.md",
            "src/b.rs",
            "docs/guide/a.md",
            "build/",
        ]));
        let rows = file_rows(&lists, FileListMode::Tree, &HashSet::new());
        assert_eq!(
            shown(&rows),
            [
                "0 docs/guide/",
                "1 a.md",
                "0 src/",
                "1 util/deep/",
                "2 x.rs",
                "1 b.rs",
                "0 README.md",
                "0 build/",
            ]
        );
    }

    #[test]
    fn ui_14_a_collapsed_folder_hides_its_files() {
        let lists = Lists::Commit(files(&["src/a.rs", "src/b.rs", "c.rs"]));
        let collapsed = HashSet::from(["commit:dir:src/".to_owned()]);
        let rows = file_rows(&lists, FileListMode::Tree, &collapsed);
        assert_eq!(shown(&rows), ["0 src/+", "0 c.rs"]);
    }

    #[test]
    fn ui_15_uncommitted_rows_lead_with_unmerged_by_full_path() {
        let lists = Lists::Uncommitted {
            staged: files(&["s/a.rs"]),
            unstaged: Vec::new(),
            unmerged: files(&["u/c.rs"]),
        };
        let rows = file_rows(&lists, FileListMode::Tree, &HashSet::new());
        assert_eq!(
            shown(&rows),
            [
                "[Unmerged 1]",
                "0 u/c.rs",
                "[Staged 1]",
                "0 s/",
                "1 a.rs",
                "[Unstaged 0]",
            ]
        );
        let clean = Lists::Uncommitted {
            staged: Vec::new(),
            unstaged: files(&["n.rs"]),
            unmerged: Vec::new(),
        };
        let rows = file_rows(&clean, FileListMode::Flat, &HashSet::new());
        assert_eq!(shown(&rows), ["[Staged 0]", "[Unstaged 1]", "0 n.rs"]);
    }

    #[test]
    fn ui_34_merged_rows_have_one_part_per_repo_with_files() {
        let (api, web) = (RepoId("api".into()), RepoId("web".into()));
        let lists = Lists::Merged(vec![
            RepoLists {
                repo: &api,
                name: "api",
                blocked: None,
                staged: files(&["a.rs"]),
                unstaged: files(&["b.rs", "c.rs"]),
                unmerged: Vec::new(),
            },
            RepoLists {
                repo: &web,
                name: "web",
                blocked: Some("in conflict"),
                staged: Vec::new(),
                unstaged: files(&["w.rs"]),
                unmerged: files(&["x.rs"]),
            },
        ]);
        let rows = file_rows(&lists, FileListMode::Flat, &HashSet::new());
        assert_eq!(
            shown(&rows),
            [
                "[Unmerged 1]",
                "<web 1>",
                "1 x.rs",
                "[Staged 1]",
                "<api 1>",
                "1 a.rs",
                "[Unstaged 3]",
                "<api 2>",
                "1 b.rs",
                "1 c.rs",
                "<web 1>",
                "1 w.rs",
            ]
        );
        // A repo's tree has its own folder keys.
        let lists = Lists::Merged(vec![RepoLists {
            repo: &api,
            name: "api",
            blocked: None,
            staged: Vec::new(),
            unstaged: files(&["src/a.rs"]),
            unmerged: Vec::new(),
        }]);
        let rows = file_rows(&lists, FileListMode::Tree, &HashSet::new());
        assert!(rows.iter().any(
            |row| matches!(row, FileRow::Folder { key, .. } if key == "api/unstaged:dir:src/")
        ));
    }

    #[test]
    fn ui_14_steps_move_between_files_only() {
        let lists = Lists::Uncommitted {
            staged: files(&["a.rs"]),
            unstaged: files(&["b.rs"]),
            unmerged: Vec::new(),
        };
        let changes = Changes {
            rows: file_rows(&lists, FileListMode::Flat, &HashSet::new()),
            ..Changes::default()
        };
        // Rows: [Staged] a.rs [Unstaged] b.rs.
        assert_eq!(changes.step(1), Some(1));
        assert_eq!(changes.end(true), Some(3));
        let mut changes = changes;
        assert!(changes.select_index(1));
        assert_eq!(changes.step(1), Some(3));
        assert_eq!(changes.step(-1), None);
        assert!(!changes.select_index(2));
    }
}
