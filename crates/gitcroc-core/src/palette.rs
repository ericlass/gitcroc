//! The command palette's contents (UI-22): the catalog of operations, and
//! the repos, groups and branches of a workspace that match a query, ranked
//! and bounded so the UI only shows the list (ARCH-6, ARCH-8).
//!
//! The catalog has one entry per operation of sections 5 and 6 and the
//! actions around them. Section 5's group forms are the same entries: they
//! act on the target set, as the toolbar does (UI-20). The labels are
//! English text here, as the core's other text is. Every entry is listed
//! whether or not it can apply now; the UI disables it by the toolbar's
//! rules. The AI entries are left out while AI features are off (AI-16).
//!
//! Matching ignores case, and every word of the query must occur in the
//! item's text: an operation's label and keywords, a repo's or group's
//! name, a branch's name and its repo's name. Items rank by how their name
//! matches the query: the whole name, the start of the name, the start of
//! a word in it, anywhere. Ties keep the order operations, repos, groups,
//! branches, each in catalog or tree order.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gitcroc_git::Refs;
use serde::{Deserialize, Serialize};

use crate::lock;
use crate::{GroupId, RepoId, SettingsStore, Trees, WorkspaceError, WorkspaceId, Workspaces};

/// The most items one answer holds (ARCH-8).
pub const MAX_PALETTE_ITEMS: usize = 100;

/// An operation of the palette. The id is stable: the UI maps each to the
/// handler its toolbar, menu or pane entry uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PaletteOp {
    AddFolder,
    Clone,
    RemoveRepo,
    OpenInFileManager,
    OpenInTerminal,
    StageFile,
    UnstageFile,
    DiscardFile,
    StageAll,
    UnstageAll,
    Commit,
    Amend,
    Checkout,
    CreateBranch,
    RenameBranch,
    DeleteBranch,
    SetUpstream,
    CheckoutRemoteBranch,
    CheckoutCommit,
    Fetch,
    FetchWithOptions,
    Pull,
    PullWithOptions,
    Push,
    PushWithOptions,
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Abort,
    Continue,
    MarkResolved,
    TakeOurs,
    TakeTheirs,
    TakeBoth,
    KeepOurs,
    KeepTheirs,
    Stash,
    StashApply,
    StashPop,
    StashDrop,
    CreateTag,
    DeleteTag,
    PushTag,
    ResetSoft,
    ResetMixed,
    ResetHard,
    AddRemote,
    RenameRemote,
    RemoveRemote,
    EditRemoteUrl,
    ResolveWithAi,
    ResolveAllWithAi,
    AcceptHighConfidence,
    Settings,
    NewWorkspace,
    ImportWorkspace,
    ExportWorkspace,
    JobList,
    CloseTab,
    NextTab,
    PreviousTab,
    NewGroup,
}

/// What an operation acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PaletteSubject {
    /// The target set (GRP-5).
    TargetSet,
    /// The selected tree row, a branch.
    Branch,
    /// The selected tree row, a tag.
    Tag,
    /// The selected tree row, a stash entry.
    Stash,
    /// The selected tree row, a remote.
    Remote,
    /// The selected commit of the history.
    Commit,
    /// The selected file of the changes pane, or the one open in the
    /// conflict editor.
    File,
    /// The repo in conflict state.
    Conflict,
    /// The app or the workspace.
    App,
}

/// One entry of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    pub op: PaletteOp,
    pub label: &'static str,
    /// More words the entry is found by.
    pub keywords: &'static [&'static str],
    pub subject: PaletteSubject,
    /// An AI entry, left out while AI features are off (AI-16).
    pub ai: bool,
}

const fn entry(
    op: PaletteOp,
    label: &'static str,
    keywords: &'static [&'static str],
    subject: PaletteSubject,
) -> PaletteEntry {
    PaletteEntry {
        op,
        label,
        keywords,
        subject,
        ai: false,
    }
}

const fn ai(
    op: PaletteOp,
    label: &'static str,
    keywords: &'static [&'static str],
    subject: PaletteSubject,
) -> PaletteEntry {
    PaletteEntry {
        ai: true,
        ..entry(op, label, keywords, subject)
    }
}

use PaletteOp as Op;
use PaletteSubject as On;

/// The catalog, in the order the empty query lists it.
const CATALOG: &[PaletteEntry] = &[
    // Repos
    entry(
        Op::AddFolder,
        "Add folder…",
        &["repository", "existing", "scan"],
        On::App,
    ),
    entry(Op::Clone, "Clone repository…", &["url"], On::App),
    entry(
        Op::RemoveRepo,
        "Remove from workspace",
        &["repository"],
        On::TargetSet,
    ),
    entry(
        Op::OpenInFileManager,
        "Open in file manager",
        &["folder", "explorer", "finder", "reveal"],
        On::TargetSet,
    ),
    entry(
        Op::OpenInTerminal,
        "Open in terminal",
        &["shell", "console"],
        On::TargetSet,
    ),
    // Working copy
    entry(Op::StageFile, "Stage file", &["add"], On::File),
    entry(Op::UnstageFile, "Unstage file", &["remove"], On::File),
    entry(
        Op::DiscardFile,
        "Discard file changes…",
        &["restore", "undo"],
        On::File,
    ),
    entry(Op::StageAll, "Stage all", &["add"], On::TargetSet),
    entry(Op::UnstageAll, "Unstage all", &[], On::TargetSet),
    entry(Op::Commit, "Commit…", &["message"], On::TargetSet),
    entry(Op::Amend, "Amend last commit…", &["message"], On::TargetSet),
    // Branches
    entry(
        Op::Checkout,
        "Check out branch…",
        &["switch", "checkout"],
        On::TargetSet,
    ),
    entry(Op::CreateBranch, "Create branch…", &["new"], On::TargetSet),
    entry(Op::RenameBranch, "Rename branch…", &["move"], On::Branch),
    entry(
        Op::DeleteBranch,
        "Delete branch…",
        &["remove"],
        On::TargetSet,
    ),
    entry(
        Op::SetUpstream,
        "Set upstream…",
        &["branch", "track"],
        On::Branch,
    ),
    entry(
        Op::CheckoutRemoteBranch,
        "Check out remote branch",
        &["switch", "checkout", "track"],
        On::Branch,
    ),
    entry(
        Op::CheckoutCommit,
        "Check out commit (detached)",
        &["switch", "checkout", "detach"],
        On::Commit,
    ),
    // Sync
    entry(Op::Fetch, "Fetch", &[], On::TargetSet),
    entry(
        Op::FetchWithOptions,
        "Fetch with options…",
        &["prune"],
        On::TargetSet,
    ),
    entry(Op::Pull, "Pull", &[], On::TargetSet),
    entry(
        Op::PullWithOptions,
        "Pull with options…",
        &["strategy", "ff-only"],
        On::TargetSet,
    ),
    entry(Op::Push, "Push", &[], On::TargetSet),
    entry(
        Op::PushWithOptions,
        "Push with options…",
        &["force", "upstream"],
        On::TargetSet,
    ),
    // Integrate
    entry(Op::Merge, "Merge into current…", &[], On::TargetSet),
    entry(Op::Rebase, "Rebase current onto…", &[], On::TargetSet),
    entry(Op::CherryPick, "Cherry-pick commit", &["apply"], On::Commit),
    entry(Op::Revert, "Revert commit", &["undo"], On::Commit),
    entry(
        Op::Abort,
        "Abort",
        &["conflict", "cancel", "merge", "rebase"],
        On::Conflict,
    ),
    entry(
        Op::Continue,
        "Continue",
        &["conflict", "merge", "rebase"],
        On::Conflict,
    ),
    entry(
        Op::MarkResolved,
        "Mark resolved",
        &["conflict", "stage"],
        On::File,
    ),
    entry(Op::TakeOurs, "Take ours", &["conflict", "hunk"], On::File),
    entry(
        Op::TakeTheirs,
        "Take theirs",
        &["conflict", "hunk"],
        On::File,
    ),
    entry(Op::TakeBoth, "Take both", &["conflict", "hunk"], On::File),
    entry(
        Op::KeepOurs,
        "Keep our version of the file",
        &["conflict", "ours"],
        On::File,
    ),
    entry(
        Op::KeepTheirs,
        "Keep their version of the file",
        &["conflict", "theirs"],
        On::File,
    ),
    // Stash
    entry(Op::Stash, "Stash changes…", &["save"], On::TargetSet),
    entry(Op::StashApply, "Apply stash", &[], On::Stash),
    entry(Op::StashPop, "Pop stash", &[], On::Stash),
    entry(Op::StashDrop, "Drop stash…", &["delete"], On::Stash),
    // Tags
    entry(Op::CreateTag, "Create tag…", &["new"], On::TargetSet),
    entry(Op::DeleteTag, "Delete tag…", &["remove"], On::Tag),
    entry(Op::PushTag, "Push tag", &[], On::Tag),
    // Reset
    entry(
        Op::ResetSoft,
        "Reset current branch: soft…",
        &[],
        On::Commit,
    ),
    entry(
        Op::ResetMixed,
        "Reset current branch: mixed…",
        &[],
        On::Commit,
    ),
    entry(
        Op::ResetHard,
        "Reset current branch: hard…",
        &["discard"],
        On::Commit,
    ),
    // Remotes
    entry(Op::AddRemote, "Add remote…", &["new"], On::TargetSet),
    entry(Op::RenameRemote, "Rename remote…", &[], On::Remote),
    entry(Op::RemoveRemote, "Remove remote…", &["delete"], On::Remote),
    entry(Op::EditRemoteUrl, "Edit remote URL…", &[], On::Remote),
    // AI
    ai(
        Op::ResolveWithAi,
        "Resolve with AI",
        &["conflict"],
        On::File,
    ),
    ai(
        Op::ResolveAllWithAi,
        "Resolve all with AI",
        &["conflict"],
        On::Conflict,
    ),
    ai(
        Op::AcceptHighConfidence,
        "Accept all high-confidence",
        &["ai", "conflict", "proposals"],
        On::File,
    ),
    // App
    entry(
        Op::Settings,
        "Settings",
        &["preferences", "options"],
        On::App,
    ),
    entry(
        Op::NewWorkspace,
        "New workspace…",
        &["tab", "create"],
        On::App,
    ),
    entry(Op::ImportWorkspace, "Import workspace…", &["open"], On::App),
    entry(
        Op::ExportWorkspace,
        "Export workspace…",
        &["save", "share"],
        On::App,
    ),
    entry(Op::JobList, "Show jobs", &["job list", "progress"], On::App),
    entry(Op::CloseTab, "Close tab", &["workspace"], On::App),
    entry(Op::NextTab, "Next tab", &["workspace"], On::App),
    entry(Op::PreviousTab, "Previous tab", &["workspace"], On::App),
    entry(Op::NewGroup, "New group…", &["create"], On::App),
];

/// The whole catalog, AI entries included, in catalog order.
pub fn palette_catalog() -> &'static [PaletteEntry] {
    CATALOG
}

/// One row of the palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaletteItem {
    Operation {
        op: PaletteOp,
        label: String,
        subject: PaletteSubject,
    },
    Repo {
        repo_id: RepoId,
        name: String,
        /// The current branch; `None` when `HEAD` is detached or the refs
        /// could not be read.
        branch: Option<String>,
    },
    Group {
        group_id: GroupId,
        name: String,
        members: u32,
    },
    Branch {
        repo_id: RepoId,
        repo_name: String,
        /// As the tree names it: `feature/x`, or `origin/feature/x` for a
        /// remote branch.
        name: String,
        /// The full ref name, for example `refs/heads/feature/x`.
        full_name: String,
        /// The remote of a remote branch; `None` for a local one.
        remote: Option<String>,
    },
}

/// The best [`MAX_PALETTE_ITEMS`] items of a query, and how many matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PaletteResult {
    pub items: Vec<PaletteItem>,
    pub total: u32,
}

/// A query of one workspace, as [`Palettes::begin`] made it the latest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteRequest(u64);

/// The latest palette query of every open workspace (ARCH-9).
#[derive(Debug, Default)]
pub struct Palettes {
    requests: Mutex<HashMap<WorkspaceId, u64>>,
}

impl Palettes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops what is kept for workspace `id`, whose tab closed (ARCH-9).
    pub fn forget(&self, id: &WorkspaceId) {
        lock(&self.requests).remove(id);
    }

    /// Makes a new query the latest of workspace `id`; older ones of the
    /// workspace are superseded.
    pub fn begin(&self, id: &WorkspaceId) -> PaletteRequest {
        let mut requests = lock(&self.requests);
        let latest = requests.entry(id.clone()).or_default();
        *latest = latest.wrapping_add(1);
        PaletteRequest(*latest)
    }

    fn is_latest(&self, id: &WorkspaceId, request: PaletteRequest) -> bool {
        lock(&self.requests).get(id) == Some(&request.0)
    }

    /// The answer to `text` for workspace `id`, or `None` when `request` is
    /// no longer its latest query. Refs not loaded yet are read, without
    /// ahead and behind, and kept as the tree keeps them.
    pub fn answer(
        &self,
        trees: &Trees,
        workspaces: &Workspaces,
        settings: &SettingsStore,
        id: &WorkspaceId,
        text: &str,
        request: PaletteRequest,
    ) -> Result<Option<PaletteResult>, WorkspaceError> {
        let view = workspaces.view(id)?;
        let query = Query::new(text);
        let ai = settings.ai().enabled;
        let mut found = Found::default();
        for known in CATALOG.iter().filter(|known| ai || !known.ai) {
            let mut words = known.label.to_owned();
            for keyword in known.keywords {
                words.push(' ');
                words.push_str(keyword);
            }
            let item = || PaletteItem::Operation {
                op: known.op,
                label: known.label.to_owned(),
                subject: known.subject,
            };
            match &query {
                None => found.push(Rank::Anywhere, item()),
                Some(query) => found.add(query, known.label, &words, item),
            }
        }
        let Some(query) = query else {
            return Ok(self.is_latest(id, request).then(|| found.result()));
        };

        // Repos in tree order: each group's members, then the ungrouped
        // repos, each repo where it first appears.
        let mut seen = HashSet::new();
        let order: Vec<&RepoId> = view
            .groups
            .iter()
            .flat_map(|group| group.repos.iter())
            .chain(view.repos.iter().map(|repo| &repo.id))
            .filter(|repo| seen.insert(*repo))
            .collect();
        let name_of = |repo: &RepoId| {
            view.repos
                .iter()
                .find(|known| &known.id == repo)
                .map_or_else(|| repo.0.clone(), |known| known.name.clone())
        };
        let mut refs: Vec<(&RepoId, String, Option<Refs>)> = Vec::new();
        for repo in order {
            if !self.is_latest(id, request) {
                return Ok(None);
            }
            // A repo whose refs cannot be read is found by its name only.
            let read = trees.read(workspaces, id, repo, false).ok();
            refs.push((repo, name_of(repo), read));
        }

        for (repo, name, read) in &refs {
            let branch = read
                .as_ref()
                .and_then(|refs| refs.local.iter().find(|branch| branch.current))
                .map(|branch| branch.name.clone());
            found.add(&query, name, name, || PaletteItem::Repo {
                repo_id: (*repo).clone(),
                name: name.clone(),
                branch,
            });
        }
        for group in &view.groups {
            found.add(&query, &group.name, &group.name, || PaletteItem::Group {
                group_id: group.id.clone(),
                name: group.name.clone(),
                members: u32::try_from(group.repos.len()).unwrap_or(u32::MAX),
            });
        }
        for (repo, repo_name, read) in &refs {
            let Some(read) = read else {
                continue;
            };
            let branch = |name: String, full_name: String, remote: Option<String>| {
                let words = format!("{name} {repo_name}");
                (name.clone(), words, move || PaletteItem::Branch {
                    repo_id: (*repo).clone(),
                    repo_name: repo_name.clone(),
                    name,
                    full_name,
                    remote,
                })
            };
            let local = read.local.iter().map(|local| {
                let full = format!("refs/heads/{}", local.name);
                branch(local.name.clone(), full, None)
            });
            let remote = read.remotes.iter().flat_map(|remote| {
                remote.branches.iter().map(|name| {
                    let full = format!("refs/remotes/{}/{name}", remote.remote);
                    branch(
                        format!("{}/{name}", remote.remote),
                        full,
                        Some(remote.remote.clone()),
                    )
                })
            });
            for (name, words, item) in local.chain(remote) {
                found.add(&query, &name, &words, item);
            }
        }
        Ok(self.is_latest(id, request).then(|| found.result()))
    }
}

/// The answer to `text` for workspace `id` (UI-22): the operations, and
/// the repos, groups and branches whose text matches, the best
/// [`MAX_PALETTE_ITEMS`] with the total. An empty query lists the
/// operations only, in catalog order. Reads off the async workers
/// (ARCH-7).
///
/// Each call supersedes the previous one for the same workspace: an older
/// call still running returns `None` (`Ok(None)`), so answers never arrive
/// out of order.
pub async fn palette_query(
    palettes: Arc<Palettes>,
    trees: Arc<Trees>,
    workspaces: Arc<Workspaces>,
    settings: Arc<SettingsStore>,
    id: WorkspaceId,
    text: String,
) -> Result<Option<PaletteResult>, WorkspaceError> {
    let request = palettes.begin(&id);
    tokio::task::spawn_blocking(move || {
        palettes.answer(&trees, &workspaces, &settings, &id, &text, request)
    })
    .await
    .map_err(WorkspaceError::Stopped)?
}

/// A query's words, lowercase, and the whole query with single spaces.
struct Query {
    words: Vec<String>,
    whole: String,
}

impl Query {
    /// `None` for a query with no words.
    fn new(text: &str) -> Option<Self> {
        let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return None;
        }
        let whole = words.join(" ");
        Some(Self { words, whole })
    }

    /// How `name` matches, when every word occurs in `words`.
    fn rank(&self, name: &str, words: &str) -> Option<Rank> {
        let words = words.to_lowercase();
        if !self.words.iter().all(|word| words.contains(word.as_str())) {
            return None;
        }
        let name = name.to_lowercase();
        if name == self.whole {
            return Some(Rank::Whole);
        }
        if name.starts_with(&self.whole) {
            return Some(Rank::Prefix);
        }
        let word_start = name.match_indices(&self.whole).any(|(at, _)| {
            name[..at]
                .chars()
                .next_back()
                .is_some_and(|before| !before.is_alphanumeric())
        });
        Some(if word_start {
            Rank::WordPrefix
        } else {
            Rank::Anywhere
        })
    }
}

/// How an item's name matches the query, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Whole,
    Prefix,
    WordPrefix,
    Anywhere,
}

/// The items found so far, in kind and catalog or tree order.
#[derive(Default)]
struct Found {
    items: Vec<(Rank, PaletteItem)>,
}

impl Found {
    fn push(&mut self, rank: Rank, item: PaletteItem) {
        self.items.push((rank, item));
    }

    fn add(&mut self, query: &Query, name: &str, words: &str, item: impl FnOnce() -> PaletteItem) {
        if let Some(rank) = query.rank(name, words) {
            self.push(rank, item());
        }
    }

    /// The best items by rank; the sort is stable, so ties keep their
    /// order.
    fn result(mut self) -> PaletteResult {
        let total = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        self.items.sort_by_key(|(rank, _)| *rank);
        self.items.truncate(MAX_PALETTE_ITEMS);
        PaletteResult {
            items: self.items.into_iter().map(|(_, item)| item).collect(),
            total,
        }
    }
}
