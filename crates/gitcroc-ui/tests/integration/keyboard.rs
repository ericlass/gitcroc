//! M6-24b: every operation of sections 5 and 6 is reachable without a
//! mouse from its own place in the window (UI-22): the toolbar, a menu, a
//! pane's control, never the palette. These tests press keys only: no
//! click and no other mouse event. What they read back is the app's state.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::history::{repo, tree_row, wait_for_element, wait_until};
use crate::palette_ops::{Did, Driver, label, shown};
use crate::support::{Fixture, main_window, settle};
use gitcroc_app::platform::{FileManager, Launch, Terminal};
use gitcroc_core::{PaletteOp, RepoId, palette_catalog};
use gitcroc_ui::changes::{FileRow, List};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{ElementId, Entity, Focusable, Role, TestAppContext, VisualTestContext};

static FILES: AtomicUsize = AtomicUsize::new(0);
static TERMS: AtomicUsize = AtomicUsize::new(0);
struct Files;
struct Term;
impl FileManager for Files {
    fn show(path: &std::path::Path) -> Launch {
        FILES.fetch_add(1, Ordering::SeqCst);
        Launch::new("git", ["--version".into()], path.to_owned())
    }
}
impl Terminal for Term {
    fn open_in(path: &std::path::Path) -> Vec<Launch> {
        TERMS.fetch_add(1, Ordering::SeqCst);
        vec![Launch::new("git", ["--version".into()], path.to_owned())]
    }
}

/// The regions F6 moves between, in order.
const REGIONS: [&str; 6] = [
    "title-region",
    "toolbar",
    "tree-list",
    "history",
    "changes",
    "status-jobs",
];

fn press(cx: &mut VisualTestContext, keys: &str) {
    crate::history::press(cx, keys);
}

pub(crate) fn secondary(key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{key}")
}

pub(crate) fn repo_node(id: &RepoId) -> String {
    format!("repo:{}", id.0)
}

/// Whether element `id` holds the focus, or something inside it does.
pub(crate) fn focused(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .is_some_and(|found| found.focused() == Some(true))
    })
}

fn selected(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .is_some_and(|found| found.selected() == Some(true))
    })
}

/// The name of the focused entry of the open menu.
fn menu_entry(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        [Role::MenuItem, Role::MenuItemCheckBox]
            .into_iter()
            .flat_map(|role| window.find_all_by_role(role))
            .find(|entry| entry.focused() == Some(true))
            .and_then(|entry| entry.label().map(str::to_owned))
    })
}

/// Whether the control whose name starts with `name` has the focus.
fn focused_named(cx: &mut VisualTestContext, name: &str) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find_all_by_role(Role::Button)
            .into_iter()
            .any(|button| {
                button.label().is_some_and(|label| label.starts_with(name))
                    && button.focused() == Some(true)
            })
    })
}

impl Driver<'_> {
    pub(crate) fn key(&mut self, keys: &str) {
        press(&mut self.cx, keys);
        settle(&mut self.cx, |_, _| true);
    }

    /// Presses F6 until the focus is in `region`.
    #[track_caller]
    pub(crate) fn f6_to(&mut self, region: &str) {
        let mut passed = Vec::new();
        for _ in 0..8 {
            if focused(&mut self.cx, region.to_owned()) {
                return;
            }
            passed.push(
                REGIONS
                    .into_iter()
                    .find(|other| focused(&mut self.cx, *other))
                    .unwrap_or("none"),
            );
            self.key("f6");
        }
        panic!("F6 never reached {region}, only {passed:?}");
    }

    /// Presses Tab until control `id` has the focus.
    #[track_caller]
    pub(crate) fn tab_to(&mut self, id: &str) {
        wait_for_element(&mut self.cx, id.to_owned().into());
        for _ in 0..150 {
            if focused(&mut self.cx, id.to_owned()) {
                return;
            }
            self.key("tab");
        }
        panic!("Tab never reached {id}");
    }

    /// Tabs to control `id` and presses Enter on it.
    #[track_caller]
    pub(crate) fn press_on(&mut self, id: &str) {
        self.tab_to(id);
        self.key("enter");
    }

    /// Selects tree row `path`'s last node with the arrow keys, from the
    /// first row, opening the rows above it with Right.
    #[track_caller]
    pub(crate) fn tree_to(&mut self, path: &[&str]) {
        self.f6_to("tree-list");
        self.key("home");
        for (at, node) in path.iter().enumerate() {
            wait_for_element(&mut self.cx, tree_row(node));
            for _ in 0..100 {
                if selected(&mut self.cx, tree_row(node)) {
                    break;
                }
                self.key("down");
            }
            assert!(selected(&mut self.cx, tree_row(node)), "row {node}");
            if let Some(next) = path.get(at + 1) {
                let expanded = self.cx.update(|window, cx| {
                    window.render_frame(cx);
                    window.find(tree_row(node)).expanded()
                });
                if expanded != Some(true) {
                    self.key("right");
                }
                wait_for_element(&mut self.cx, tree_row(next));
            }
        }
    }

    /// Selects history row `row` with the arrow keys, from the first.
    #[track_caller]
    pub(crate) fn history_to(&mut self, row: &str) {
        let id: ElementId = row.to_owned().into();
        wait_for_element(&mut self.cx, id.clone());
        self.f6_to("history");
        self.key("home");
        for _ in 0..40 {
            if selected(&mut self.cx, id.clone()) {
                return;
            }
            self.key("down");
        }
        panic!("history row {row}");
    }

    /// The changes pane's row index of `path` in `list`.
    pub(crate) fn file_row(&mut self, list: List, path: &str) -> usize {
        crate::stage::row_of(&mut self.cx, &self.window, list, path)
    }

    /// The changes pane's row index of `list`'s heading.
    pub(crate) fn section_row(&mut self, list: List) -> usize {
        crate::changes::with_changes(&mut self.cx, &self.window, |changes| {
            changes.rows().iter().position(
                |row| matches!(row, FileRow::Section { list: shown, .. } if *shown == list),
            )
        })
        .unwrap_or_else(|| panic!("no section {list:?}"))
    }

    /// Selects the changed file `path` of `list` with the arrow keys.
    #[track_caller]
    pub(crate) fn file_to(&mut self, list: List, path: &str) {
        // A post-write list can arrive before the job ends. Wait for the
        // repo to accept the next operation before navigating its rows.
        self.settled();
        let window = self.window.clone();
        wait_until(&mut self.cx, |cx| {
            crate::changes::with_changes(cx, &window, |changes| {
                !changes.loading() && changes.file().is_some()
            })
        });
        let at = self.file_row(list, path);
        // F6 lands on the file list, not in a view beside it.
        let window = self.window.clone();
        let on_list = |cx: &mut VisualTestContext| {
            cx.update(|w, cx| {
                let pane = window.read(cx).changes(cx).unwrap();
                gpui_kit::Focusable::focus_handle(pane.read(cx), cx).is_focused(w)
            })
        };
        for _ in 0..8 {
            if on_list(&mut self.cx) {
                break;
            }
            self.key("f6");
        }
        assert!(on_list(&mut self.cx), "F6 never reached the file list");
        self.key("home");
        for _ in 0..40 {
            if selected(&mut self.cx, crate::changes::row(at)) {
                return;
            }
            self.key("down");
        }
        panic!("file {path} in {list:?}");
    }

    /// Moves the focus in the open menu to `entry` with the arrow keys and
    /// presses Enter.
    #[track_caller]
    pub(crate) fn choose_in_menu(&mut self, entry: &str) {
        wait_until(&mut self.cx, |cx| menu_entry(cx).is_some());
        for _ in 0..30 {
            if menu_entry(&mut self.cx).as_deref() == Some(entry) {
                self.key("enter");
                return;
            }
            self.key("down");
        }
        panic!("no menu entry {entry}: at {:?}", menu_entry(&mut self.cx));
    }

    /// Opens the menu of the focused row with Shift+F10 and chooses `entry`.
    #[track_caller]
    pub(crate) fn menu(&mut self, entry: &str) {
        self.key("shift-f10");
        self.choose_in_menu(entry);
    }

    /// Reaches `op` by `entry`, keys only, and checks that it did what
    /// `did` says.
    pub(crate) fn reach(&mut self, op: PaletteOp, entry: impl FnOnce(&mut Self), did: Did) {
        let jobs = self.jobs();
        entry(self);
        self.expect(label(op), jobs, did);
    }

    /// Presses toolbar button `id`.
    #[track_caller]
    pub(crate) fn in_toolbar(&mut self, id: &str) {
        self.f6_to("toolbar");
        self.press_on(id);
    }

    /// Opens the tab bar's "+" menu and chooses `entry`.
    #[track_caller]
    pub(crate) fn add_tab(&mut self, entry: &str) {
        self.f6_to("title-region");
        self.press_on("tab-add");
        self.choose_in_menu(entry);
    }

    /// Opens the tree's add menu and chooses `entry`.
    #[track_caller]
    pub(crate) fn add_repo(&mut self, entry: &str) {
        self.f6_to("tree-list");
        self.press_on("tree-add");
        self.choose_in_menu(entry);
    }

    pub(crate) fn active(&mut self) -> Option<gitcroc_core::WorkspaceId> {
        let window = self.window.clone();
        self.cx.update(|_, cx| window.read(cx).tabs(cx).active)
    }

    pub(crate) fn open_tabs(&mut self) -> usize {
        let window = self.window.clone();
        self.cx.update(|_, cx| window.read(cx).tabs(cx).open.len())
    }

    /// Checks out `branch` with git, outside the app, and waits until the
    /// tree has read it: until then the app still has the branch before as
    /// the current one, and checking that one out again does nothing.
    pub(crate) fn checkout_outside(&mut self, r: &repo_gen::Repo, repo: &RepoId, branch: &str) {
        r.git(["checkout", "--quiet", branch]).unwrap();
        self.f.refresh(&self.f.active());
        let window = self.window.clone();
        wait_until(&mut self.cx, |cx| {
            cx.update(|_, cx| {
                let tree = window.read(cx).tree(cx).unwrap();
                let summary = tree.read(cx).tree().summary(repo).cloned();
                matches!(
                    summary.and_then(|view| view.summary).map(|summary| summary.head),
                    Some(gitcroc_core::Head::Branch { name, .. }) if name == branch
                )
            })
        });
    }

    /// Ends the cherry-pick an already applied commit left in progress, and
    /// waits until the tree no longer shows the repo blocked by it, so its
    /// menus enable again.
    pub(crate) fn abort_cherry_pick(&mut self, r: &repo_gen::Repo, repo: &RepoId) {
        let _ = r.git(["cherry-pick", "--abort"]);
        self.f.refresh(&self.f.active());
        let window = self.window.clone();
        wait_until(&mut self.cx, |cx| {
            cx.update(|_, cx| {
                let tree = window.read(cx).tree(cx).unwrap();
                let tree = tree.read(cx).tree();
                tree.summary(repo).is_some()
                    && gitcroc_ui::row_context::blocked(tree.summary(repo)).is_none()
            })
        });
    }

    /// The commits the history has marked together.
    pub(crate) fn marked(&mut self) -> usize {
        let window = self.window.clone();
        self.cx.update(|_, cx| {
            window
                .read(cx)
                .history(cx)
                .map(|history| history.read(cx).marked().len())
                .unwrap_or_default()
        })
    }

    /// The uncommitted changes of `node`, reached by keys.
    #[track_caller]
    pub(crate) fn uncommitted(&mut self, node: &str) {
        self.tree_to(&["ungrouped", node]);
        self.history_to("history-uncommitted");
        crate::changes::wait_for_view(&mut self.cx, &self.window.clone());
    }

    /// Unmerged file `path` open in its editor, with the focus in it.
    #[track_caller]
    pub(crate) fn editor_on(&mut self, path: &str) -> Entity<gitcroc_ui::conflict::ConflictEditor> {
        self.file_to(List::Unmerged, path);
        self.key("enter");
        let editor = crate::conflict_flow::wait_for_editor(&mut self.cx, &self.window, path);
        let text = editor.clone();
        wait_until(&mut self.cx, |cx| {
            cx.update(|window, cx| text.read(cx).editor.focus_handle(cx).is_focused(window))
        });
        editor
    }
}

pub(crate) fn driver<'a>(f: &'a Fixture, root: &mut TestAppContext) -> Driver<'a> {
    let (mut cx, window) = main_window(root, f);
    crate::stage::enlarge(&mut cx);
    Driver {
        f,
        cx,
        window,
        done: BTreeSet::new(),
    }
}

#[gpui::test]
fn ui_22_every_operation_reachable_without_a_mouse(root: &mut TestAppContext) {
    use PaletteOp as Op;
    let mut done = BTreeSet::new();

    // The toolbar, the tree, the history and the window, on a repo with an
    // origin, a branch, a tag and two stashes.
    {
        let mut setup = crate::group_ops::Setup::new(&[
            ("api", crate::group_ops::Remote::Ok),
            ("web", crate::group_ops::Remote::Ok),
        ]);
        setup.fixture.services = setup
            .fixture
            .services
            .clone()
            .with_repo_openers::<Files, Term>();
        let api = repo(&setup.made, "api");
        api.branch("topic").unwrap();
        api.git(["tag", "v1"]).unwrap();
        for (path, text) in [("a.txt", "a\n"), ("b.txt", "b\n")] {
            api.write(path, text).unwrap();
            api.git(["add", "--", path]).unwrap();
            api.git(["stash", "--quiet"]).unwrap();
        }
        api.commit_file("second.txt", "second\n", "Second").unwrap();
        api.commit_file("third.txt", "third\n", "Third").unwrap();
        setup.add();
        let api_id = setup.id("api");
        let node = repo_node(&api_id);
        let web = repo_node(&setup.id("web"));
        let f = &setup.fixture;
        let mut d = driver(f, root);
        d.tree_to(&["ungrouped", &node]);
        // The toolbar.
        for (op, id) in [
            (Op::Fetch, "toolbar-0"),
            (Op::Pull, "toolbar-1"),
            (Op::Push, "toolbar-2"),
        ] {
            d.reach(op, |d| d.in_toolbar(id), Did::Runs);
        }
        for (op, id) in [
            (Op::FetchWithOptions, "toolbar-options-0"),
            (Op::PullWithOptions, "toolbar-options-1"),
            (Op::PushWithOptions, "toolbar-options-2"),
            (Op::Merge, "toolbar-4"),
            (Op::Stash, "toolbar-5"),
        ] {
            d.reach(op, |d| d.in_toolbar(id), Did::Dialog("operation-dialog"));
        }
        d.reach(
            Op::Rebase,
            |d| {
                d.in_toolbar("toolbar-4");
                d.tab_to("op-rebase");
                d.key("space");
                let checked = d.cx.update(|window, cx| {
                    window.render_frame(cx);
                    window.find("op-rebase").checked()
                });
                assert_eq!(checked, Some(true));
            },
            Did::Dialog("operation-dialog"),
        );
        d.reach(
            Op::CreateBranch,
            |d| d.in_toolbar("toolbar-3"),
            Did::Dialog("branch-dialog"),
        );
        d.reach(
            Op::Settings,
            |d| d.in_toolbar("toolbar-settings"),
            Did::Dialog("settings-dialog"),
        );
        // The status bar, the tab bar's "+" and the tree's add menu.
        d.reach(
            Op::JobList,
            |d| {
                d.f6_to("status-jobs");
                d.key("enter");
            },
            Did::Dialog("job-list"),
        );
        d.reach(
            Op::NewWorkspace,
            |d| d.add_tab("New workspace…"),
            Did::Dialog("name-dialog"),
        );
        d.reach(
            Op::Clone,
            |d| d.add_repo("Clone…"),
            Did::Dialog("clone-dialog"),
        );
        // The tree's menus.
        d.reach(
            Op::NewGroup,
            |d| {
                d.tree_to(&["ungrouped"]);
                d.menu("New group…");
            },
            Did::Dialog("name-dialog"),
        );
        d.tree_to(&["ungrouped", &node]);
        d.reach(
            Op::CreateTag,
            |d| d.menu("New tag…"),
            Did::Dialog("operation-dialog"),
        );
        d.reach(
            Op::OpenInFileManager,
            |d| d.menu("Open in file manager"),
            Did::Check(Box::new(|_| FILES.load(Ordering::SeqCst) > 0)),
        );
        d.reach(
            Op::OpenInTerminal,
            |d| d.menu("Open in terminal"),
            Did::Check(Box::new(|_| TERMS.load(Ordering::SeqCst) > 0)),
        );
        // The commit menu, on the selected commit and on two marked with
        // Shift or with Ctrl/Cmd+Space (UI-13).
        for (op, choice) in [
            (Op::ResetSoft, "op-choice-0"),
            (Op::ResetMixed, "op-choice-1"),
            (Op::ResetHard, "op-choice-2"),
        ] {
            d.reach(
                op,
                |d| {
                    d.history_to("history-row-1");
                    d.menu("Reset current branch to here…");
                    d.press_on(choice);
                },
                Did::Dialog("operation-dialog"),
            );
        }
        d.history_to("history-row-0");
        d.key("shift-down");
        assert_eq!(d.marked(), 2);
        d.reach(
            Op::CherryPick,
            |d| d.menu("Cherry-pick 2 commits"),
            Did::Runs,
        );
        d.abort_cherry_pick(&api, &api_id);
        d.history_to("history-row-0");
        d.key(&secondary("down"));
        d.key(&secondary("down"));
        // The first stays marked; the third joins it.
        assert_eq!(d.marked(), 1);
        d.key(&secondary("space"));
        assert_eq!(d.marked(), 2);
        d.reach(
            Op::CherryPick,
            |d| d.menu("Cherry-pick 2 commits"),
            Did::Runs,
        );
        d.abort_cherry_pick(&api, &api_id);
        d.history_to("history-row-1");
        d.reach(Op::CherryPick, |d| d.menu("Cherry-pick"), Did::Runs);
        d.abort_cherry_pick(&api, &api_id);
        d.history_to("history-row-0");
        d.reach(Op::Revert, |d| d.menu("Revert"), Did::Runs);
        d.history_to("history-row-1");
        d.reach(
            Op::CheckoutCommit,
            |d| d.menu("Check out (detached)"),
            Did::Runs,
        );
        d.checkout_outside(&api, &api_id, "main");
        // The tree's rows.
        let tags = format!("{node}:tags");
        let tag = format!("{tags}:ref:v1");
        d.tree_to(&["ungrouped", &node, &tags, &tag]);
        d.reach(
            Op::DeleteTag,
            |d| d.menu("Delete…"),
            Did::Dialog("operation-dialog"),
        );
        d.reach(Op::PushTag, |d| d.menu("Push"), Did::Runs);
        let stashes = format!("{node}:stashes");
        let stash = format!("{stashes}:ref:0");
        for (op, entry) in [
            (Op::StashApply, "Apply"),
            (Op::StashDrop, "Drop…"),
            (Op::StashPop, "Pop"),
        ] {
            d.reach(
                op,
                |d| {
                    d.tree_to(&["ungrouped", &node, &stashes, &stash]);
                    d.menu(entry);
                },
                Did::Runs,
            );
        }
        let local = format!("{node}:local");
        let topic = format!("{local}:ref:topic");
        d.tree_to(&["ungrouped", &node, &local, &topic]);
        for (op, entry) in [
            (Op::RenameBranch, "Rename…"),
            (Op::DeleteBranch, "Delete…"),
            (Op::SetUpstream, "Set upstream…"),
        ] {
            d.reach(op, |d| d.menu(entry), Did::Dialog("branch-dialog"));
        }
        d.reach(Op::Checkout, |d| d.menu("Check out"), Did::Runs);
        d.checkout_outside(&api, &api_id, "main");
        // Enter on a branch checks it out, as a double-click does (UI-5).
        d.reach(
            Op::Checkout,
            |d| {
                d.tree_to(&["ungrouped", &node, &local, &topic]);
                d.key("enter");
            },
            Did::Runs,
        );
        assert_eq!(
            api.git(["branch", "--show-current"]).unwrap().trim(),
            "topic"
        );
        d.checkout_outside(&api, &api_id, "main");
        let remotes = format!("{node}:remotes");
        let origin = format!("{remotes}:origin");
        let origin_main = format!("{origin}:ref:main");
        d.reach(
            Op::CheckoutRemoteBranch,
            |d| {
                d.tree_to(&["ungrouped", &node, &remotes, &origin, &origin_main]);
                d.menu("Check out");
            },
            Did::Runs,
        );
        d.tree_to(&["ungrouped", &node, &remotes]);
        d.reach(
            Op::AddRemote,
            |d| d.menu("Add remote…"),
            Did::Dialog("operation-dialog"),
        );
        d.tree_to(&["ungrouped", &node, &remotes, &origin]);
        d.reach(
            Op::RenameRemote,
            |d| d.menu("Rename…"),
            Did::Dialog("operation-dialog"),
        );
        d.reach(
            Op::EditRemoteUrl,
            |d| d.menu("Edit URL…"),
            Did::Dialog("operation-dialog"),
        );
        d.reach(Op::RemoveRemote, |d| d.menu("Remove…"), Did::Runs);
        // The tabs, and the workspace's file.
        let first = f.active();
        let app = f.app().clone();
        f.runtime
            .block_on(app.workspace_create("Second".to_owned()))
            .unwrap();
        wait_until(&mut d.cx, |cx| {
            cx.update(|_, cx| d.window.read(cx).tabs(cx).open.len()) == 2
        });
        let second = d.active().unwrap();
        assert_ne!(second, first);
        wait_until(&mut d.cx, |_| f.active() == second);
        d.f6_to("title-region");
        let back = first.clone();
        d.reach(
            Op::PreviousTab,
            |d| d.key("left"),
            Did::Check(Box::new({
                let window = d.window.clone();
                move |cx| cx.update(|_, cx| window.read(cx).tabs(cx).active.as_ref() == Some(&back))
            })),
        );
        let ahead = second.clone();
        d.reach(
            Op::NextTab,
            |d| d.key("right"),
            Did::Check(Box::new({
                let window = d.window.clone();
                move |cx| {
                    cx.update(|_, cx| window.read(cx).tabs(cx).active.as_ref() == Some(&ahead))
                }
            })),
        );
        let file = setup.made.path().join("second.gitcroc.json");
        f.answer(Some(&file));
        let written = file.clone();
        d.reach(
            Op::ExportWorkspace,
            |d| {
                d.f6_to("title-region");
                d.menu("Export workspace…");
            },
            Did::Check(Box::new(move |_| written.exists())),
        );
        d.reach(
            Op::CloseTab,
            |d| {
                d.f6_to("title-region");
                d.key("delete");
            },
            Did::Check(Box::new({
                let window = d.window.clone();
                move |cx| cx.update(|_, cx| window.read(cx).tabs(cx).open.len()) == 1
            })),
        );
        f.answer(Some(&file));
        d.reach(
            Op::ImportWorkspace,
            |d| d.add_tab("Import workspace…"),
            Did::Check(Box::new({
                let window = d.window.clone();
                move |cx| cx.update(|_, cx| window.read(cx).tabs(cx).open.len()) == 2
            })),
        );
        // Back to the first tab, by its key.
        for _ in 0..3 {
            if d.active().as_ref() == Some(&first) {
                break;
            }
            d.key(&secondary("pageup"));
        }
        assert_eq!(d.active().as_ref(), Some(&first));
        wait_until(&mut d.cx, |_| f.active() == first);
        assert_eq!(d.open_tabs(), 2);
        // The tree's add menu, and Remove from workspace.
        let extra = crate::history::repos(&["extra"]);
        f.answer(Some(extra.path()));
        let app = f.app().clone();
        let workspace = first.clone();
        let names = move || {
            app.workspace_get()
                .open
                .into_iter()
                .find(|view| view.id == workspace)
                .map(|view| view.repos.len())
                .unwrap_or_default()
        };
        let before = names();
        let grown = names.clone();
        d.reach(
            Op::AddFolder,
            |d| d.add_repo("Add folder…"),
            Did::Check(Box::new(move |_| grown() > before)),
        );
        let after = names();
        d.reach(
            Op::RemoveRepo,
            |d| {
                d.tree_to(&["ungrouped", &web]);
                d.menu("Remove from workspace");
            },
            Did::Check(Box::new(move |_| names() < after)),
        );
        done.extend(d.done);
    }

    // The changes pane's lists and the commit box.
    {
        let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
        let made = crate::history::repos(&["api"]);
        let api = repo(&made, "api");
        api.git(["config", "user.name", "Test User"]).unwrap();
        api.git(["config", "user.email", "test@example.com"])
            .unwrap();
        api.write("README.md", "changed\n").unwrap();
        api.write("new.txt", "new\n").unwrap();
        let added = crate::history::add(&f, &made);
        let node = repo_node(&added[0].1);
        let mut d = driver(&f, root);
        d.uncommitted(&node);
        crate::stage::wait_for_lists(&mut d.cx, &d.window, &[], &["README.md", "new.txt"]);
        let lists = |d: &mut Driver, staged: &[&str], unstaged: &[&str]| {
            crate::stage::wait_for_lists(&mut d.cx, &d.window, staged, unstaged);
        };
        d.reach(
            Op::StageFile,
            |d| {
                d.file_to(List::Unstaged, "new.txt");
                d.key("space");
            },
            Did::Runs,
        );
        lists(&mut d, &["new.txt"], &["README.md"]);
        d.reach(
            Op::UnstageFile,
            |d| {
                d.file_to(List::Staged, "new.txt");
                d.key("enter");
            },
            Did::Runs,
        );
        lists(&mut d, &[], &["README.md", "new.txt"]);
        // On macOS the key marked delete is Backspace (GRP-9: it asks first).
        let delete = if cfg!(target_os = "macos") {
            "backspace"
        } else {
            "delete"
        };
        d.reach(
            Op::DiscardFile,
            |d| {
                d.file_to(List::Unstaged, "README.md");
                d.key(delete);
            },
            Did::Dialog("discard-dialog"),
        );
        let all = |d: &mut Driver, list: List| {
            let at = d.section_row(list);
            d.f6_to("changes");
            d.press_on(&format!("changes-row-{at}-all"));
        };
        d.reach(Op::StageAll, |d| all(d, List::Unstaged), Did::Runs);
        lists(&mut d, &["README.md", "new.txt"], &[]);
        d.reach(Op::UnstageAll, |d| all(d, List::Staged), Did::Runs);
        lists(&mut d, &[], &["README.md", "new.txt"]);
        all(&mut d, List::Unstaged);
        lists(&mut d, &["README.md", "new.txt"], &[]);
        // A change left out keeps the changes, and the commit box, shown.
        api.write("left.txt", "left\n").unwrap();
        lists(&mut d, &["README.md", "new.txt"], &["left.txt"]);
        d.reach(
            Op::Commit,
            |d| {
                d.tab_to("commit-subject");
                d.cx.update(|window, cx| window.input("From the keyboard", cx));
                d.cx.run_until_parked();
                d.press_on("commit-commit");
            },
            Did::Runs,
        );
        // Amend first fills in the last message, as its button does.
        let w = d.window.clone();
        d.reach(
            Op::Amend,
            |d| d.press_on("commit-amend"),
            Did::Check(Box::new(move |cx| {
                cx.update(|_, cx| {
                    let changes = w.read(cx).changes(cx).unwrap();
                    let commit_box = changes.read(cx).commit_box().read(cx);
                    commit_box
                        .draft()
                        .is_some_and(|draft| draft.subject == "From the keyboard")
                })
            })),
        );
        done.extend(d.done);
    }

    // The conflict editor, the banner and AI.
    {
        use crate::ai_resolve::{mock_provider, resolving, start};
        let f = start(crate::support::AiScript {
            replies: vec![resolving(&[(1, "both\n")]), resolving(&[(1, "both\n")])],
            ..crate::support::AiScript::default()
        });
        mock_provider(&f, "Mock", true);
        let made = crate::history::repos(&["api"]);
        let r = repo(&made, "api");
        crate::conflict_flow::merge_conflict(&r, &["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]);
        let added = crate::history::add(&f, &made);
        let node = repo_node(&added[0].1);
        let mut d = driver(&f, root);
        d.uncommitted(&node);
        // F7 puts the focus on the first conflict's Take ours; Tab goes on
        // to Take theirs and Take both.
        for (path, op, tabs) in [
            ("a.txt", Op::TakeOurs, 0),
            ("b.txt", Op::TakeTheirs, 1),
            ("c.txt", Op::TakeBoth, 2),
        ] {
            let editor = d.editor_on(path);
            let before = crate::conflict_flow::value(&mut d.cx, &editor);
            d.reach(
                op,
                |d| {
                    d.key("f7");
                    assert!(focused_named(&mut d.cx, "Take ours in conflict 1"));
                    for _ in 0..tabs {
                        d.key("tab");
                    }
                    d.key("enter");
                },
                Did::Check(Box::new(move |cx| {
                    crate::conflict_flow::value(cx, &editor) != before
                })),
            );
        }
        d.reach(
            Op::MarkResolved,
            |d| {
                d.editor_on("a.txt");
                d.press_on("conflict-mark-resolved");
            },
            Did::Runs,
        );
        let editor = d.editor_on("d.txt");
        d.reach(
            Op::ResolveWithAi,
            |d| d.press_on("conflict-resolve-ai"),
            Did::Runs,
        );
        let proposed = editor.clone();
        wait_until(&mut d.cx, |cx| {
            crate::ai_resolve::proposed(cx, &proposed) > 0
        });
        d.reach(
            Op::AcceptHighConfidence,
            |d| d.press_on("conflict-accept-high"),
            Did::Check(Box::new(move |cx| {
                !crate::conflict_flow::value(cx, &editor).contains("<<<<<<<")
            })),
        );
        // The banner's buttons: F6 to the history, then Tab.
        d.reach(
            Op::ResolveAllWithAi,
            |d| {
                d.f6_to("history");
                d.press_on("conflict-banner-resolve-ai");
            },
            Did::Runs,
        );
        d.settled();
        d.reach(
            Op::Abort,
            |d| {
                d.f6_to("history");
                d.press_on("conflict-banner-abort");
            },
            Did::Runs,
        );
        let _ = r;
        done.extend(d.done);
    }

    // Continue, once every file is resolved.
    {
        let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
        let made = crate::history::repos(&["api"]);
        let r = repo(&made, "api");
        r.git(["config", "user.name", "Test User"]).unwrap();
        r.git(["config", "user.email", "test@example.com"]).unwrap();
        crate::conflict_flow::merge_conflict(&r, &["a.txt"]);
        r.write("a.txt", "resolved\n").unwrap();
        r.git(["add", "--", "a.txt"]).unwrap();
        let added = crate::history::add(&f, &made);
        let node = repo_node(&added[0].1);
        let mut d = driver(&f, root);
        d.tree_to(&["ungrouped", &node]);
        wait_for_element(&mut d.cx, "conflict-banner".into());
        d.reach(
            Op::Continue,
            |d| {
                d.f6_to("history");
                d.press_on("conflict-banner-continue");
            },
            Did::Runs,
        );
        done.extend(d.done);
    }

    // The choice of version (OPS-6).
    {
        let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
        let made = crate::history::repos(&["api"]);
        let r = repo(&made, "api");
        r.git(["config", "user.name", "Test User"]).unwrap();
        r.git(["config", "user.email", "test@example.com"]).unwrap();
        r.git(["config", "core.autocrlf", "false"]).unwrap();
        for (side, text) in [
            ("base", b"\0base"),
            ("main", b"\0main"),
            ("topic", b"\0tpic"),
        ] {
            for path in ["f.bin", "g.bin"] {
                r.write(path, text.as_slice()).unwrap();
            }
            r.git(["add", "--", "f.bin", "g.bin"]).unwrap();
            r.git(["commit", "--quiet", "-m", side]).unwrap();
            if side == "base" {
                r.branch("topic").unwrap();
                r.git(["checkout", "--quiet", "-b", "side"]).unwrap();
                r.git(["checkout", "--quiet", "main"]).unwrap();
            }
            if side == "main" {
                r.git(["checkout", "--quiet", "topic"]).unwrap();
            }
        }
        r.git(["checkout", "--quiet", "main"]).unwrap();
        r.merge_with_conflicts("topic").unwrap();
        let added = crate::history::add(&f, &made);
        let node = repo_node(&added[0].1);
        let mut d = driver(&f, root);
        d.uncommitted(&node);
        for (path, op, control) in [
            ("f.bin", Op::KeepOurs, "conflict-use-ours"),
            ("g.bin", Op::KeepTheirs, "conflict-use-theirs"),
        ] {
            d.reach(
                op,
                |d| {
                    d.file_to(List::Unmerged, path);
                    // Enter moves to the choice's first button.
                    d.key("enter");
                    d.press_on(control);
                },
                Did::Runs,
            );
        }
        done.extend(d.done);
    }

    // Check for updates, with a version on offer.
    {
        let (source, test) = crate::update::offer();
        let f = Fixture::start_with(crate::support::Options {
            updates: Some((source, test)),
            ..crate::support::Options::default()
        });
        let made = crate::history::repos(&["api"]);
        crate::history::add(&f, &made);
        let mut d = driver(&f, root);
        f.app().update_check_now();
        wait_for_element(&mut d.cx, "update-notice".into());
        d.reach(
            Op::CheckForUpdates,
            |d| {
                d.f6_to("status-jobs");
                d.press_on("update-notice");
            },
            Did::Dialog("update-dialog"),
        );
        done.extend(d.done);
    }

    let all: BTreeSet<String> = palette_catalog()
        .iter()
        .map(|entry| entry.label.to_owned())
        .collect();
    let missing: Vec<&String> = all.difference(&done).collect();
    assert!(missing.is_empty(), "not reached by keys: {missing:?}");
}

#[gpui::test]
fn ui_22_settings_opens_with_its_shortcut(root: &mut TestAppContext) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let made = crate::history::repos(&["api"]);
    crate::history::add(&f, &made);
    let mut d = driver(&f, root);
    d.f6_to("tree-list");
    d.key(&secondary(","));
    wait_for_element(&mut d.cx, "settings-dialog".into());
    // Escape gives the focus back.
    d.key("escape");
    assert!(!shown(&mut d.cx, "settings-dialog"));
    assert!(focused(&mut d.cx, "tree-list"));
    // Not while another dialog is open.
    d.f6_to("status-jobs");
    d.key("enter");
    wait_for_element(&mut d.cx, "job-list".into());
    d.key(&secondary(","));
    assert!(shown(&mut d.cx, "job-list"));
    assert!(!shown(&mut d.cx, "settings-dialog"));
    d.key("escape");
    // The palette's Settings row shows the keys.
    let window = d.window.clone();
    crate::palette::open_palette(&mut d.cx, &window);
    crate::palette::search(&mut d.cx, &window, "Settings", "Settings");
    let keys = gitcroc_ui::keys::palette_shortcut(PaletteOp::Settings).unwrap();
    assert_eq!(
        crate::history::label_of(&mut d.cx, ElementId::Name("palette-row-0-shortcut".into())),
        Some(keys.label(cfg!(target_os = "macos")))
    );
}

/// The menu entry that has the focus, by its place.
fn menu_at(cx: &mut VisualTestContext) -> Option<usize> {
    (0..40).find(|at| focused(cx, ElementId::NamedInteger("menu-item".into(), *at as u64)))
}

fn menu_len(cx: &mut VisualTestContext) -> usize {
    cx.update(|window, cx| {
        window.render_frame(cx);
        (0..40)
            .take_while(|at| {
                window
                    .try_find(ElementId::NamedInteger("menu-item".into(), *at as u64))
                    .is_some()
            })
            .count()
    })
}

#[gpui::test]
fn ui_22_menus_move_with_the_arrow_keys_and_give_the_focus_back_on_escape(
    root: &mut TestAppContext,
) {
    let f = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let made = crate::history::repos(&["api"]);
    let added = crate::history::add(&f, &made);
    let node = repo_node(&added[0].1);
    let mut d = driver(&f, root);
    // A repo row's menu: every entry is enabled, and the keys wrap around.
    d.tree_to(&["ungrouped", &node]);
    d.key("shift-f10");
    wait_until(&mut d.cx, |cx| menu_at(cx).is_some());
    assert_eq!(menu_at(&mut d.cx), Some(0));
    let last = menu_len(&mut d.cx) - 1;
    d.key("end");
    assert_eq!(menu_at(&mut d.cx), Some(last));
    d.key("down");
    assert_eq!(menu_at(&mut d.cx), Some(0));
    d.key("up");
    assert_eq!(menu_at(&mut d.cx), Some(last));
    d.key("home");
    assert_eq!(menu_at(&mut d.cx), Some(0));
    d.key("escape");
    assert!(!shown(&mut d.cx, "menu"));
    assert!(focused(&mut d.cx, "tree-list"));
    // The commit menu, from the history.
    d.history_to("history-row-0");
    d.key("menu");
    wait_until(&mut d.cx, |cx| menu_at(cx).is_some());
    d.key("escape");
    assert!(!shown(&mut d.cx, "menu"));
    assert!(focused(&mut d.cx, "history"));
    // The tab's menu, and the tab bar's "+".
    let tab = ElementId::Name(format!("tab-{}", f.active().0).into());
    d.f6_to("title-region");
    assert!(focused(&mut d.cx, tab.clone()));
    d.key("shift-f10");
    wait_until(&mut d.cx, |cx| menu_at(cx).is_some());
    assert_eq!(menu_at(&mut d.cx), Some(0));
    d.key("escape");
    assert!(focused(&mut d.cx, tab));
    d.press_on("tab-add");
    wait_until(&mut d.cx, |cx| menu_at(cx).is_some());
    assert_eq!(menu_at(&mut d.cx), Some(0));
    d.key("escape");
    assert!(focused(&mut d.cx, "tab-add"));
    // The tree's add menu.
    d.f6_to("tree-list");
    d.press_on("tree-add");
    wait_until(&mut d.cx, |cx| menu_at(cx).is_some());
    d.key("escape");
    assert!(focused(&mut d.cx, "tree-add"));
}

#[gpui::test]
fn ui_22_the_job_list_a_jobs_output_and_its_report_are_reachable_by_keyboard(
    root: &mut TestAppContext,
) {
    let mut setup = crate::group_ops::Setup::new(&[
        ("api", crate::group_ops::Remote::Ok),
        ("docs", crate::group_ops::Remote::None),
    ]);
    // A push whose hook says more than the output shows at once.
    let api = repo(&setup.made, "api");
    api.commit_file("next.txt", "next\n", "Next").unwrap();
    let hook = api.path().join(".git/hooks/pre-push");
    std::fs::write(
        &hook,
        "#!/bin/sh\nfor n in $(seq 1 200); do echo \"hook line $n\" >&2; done\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    setup.add();
    let node = repo_node(&setup.id("api"));
    let f = &setup.fixture;
    let mut d = driver(f, root);
    d.tree_to(&["ungrouped", &node]);
    let jobs = d.jobs();
    d.in_toolbar("toolbar-0");
    d.expect("Fetch", jobs, Did::Runs);
    let jobs = d.jobs();
    d.in_toolbar("toolbar-2");
    d.expect("Push", jobs, Did::Runs);

    // The status bar opens the list, with the focus on it.
    d.f6_to("status-jobs");
    d.key("enter");
    wait_for_element(&mut d.cx, "job-list".into());
    assert!(focused(&mut d.cx, "job-rows"));
    let row = |at: u64| ElementId::Name(format!("job-row-{at}").into());
    d.key("home");
    assert!(selected(&mut d.cx, row(0)));
    d.key("down");
    assert!(selected(&mut d.cx, row(1)));
    d.key("up");
    assert!(selected(&mut d.cx, row(0)));
    // The jobs are newest first: the push, the fetch, adding the repos.
    d.key("end");
    assert!(selected(&mut d.cx, row(2)));
    // The push: Tab reaches its output, which the keys scroll.
    d.key("home");
    let output = crate::history::label_of(&mut d.cx, "job-output".into()).unwrap_or_default();
    assert!(output.contains("hook line 200"), "{output}");
    d.tab_to("job-output");
    let offset = |d: &mut Driver| {
        let window = d.window.clone();
        d.cx.update(|_, cx| {
            window
                .read(cx)
                .dialog_of::<gitcroc_ui::job_list::JobList>()
                .unwrap()
                .read(cx)
                .output_offset()
        })
    };
    assert_eq!(offset(&mut d), gpui_kit::px(0.));
    d.key("down");
    let step = offset(&mut d);
    assert!(step > gpui_kit::px(0.));
    d.key("pagedown");
    assert!(offset(&mut d) > step);
    d.key("end");
    let end = offset(&mut d);
    d.key("down");
    assert_eq!(offset(&mut d), end, "past the end");
    d.key("home");
    assert_eq!(offset(&mut d), gpui_kit::px(0.));
    d.key("escape");
    assert!(!shown(&mut d.cx, "job-list"));
    assert!(focused(&mut d.cx, "status-jobs"));

    // A group job's report: its lines' controls move with the arrow keys
    // and open each line's output.
    let group = setup.group("Backend", &["api", "docs"]);
    let group_node = setup.group_node(&group);
    d.tree_to(&[&group_node]);
    d.in_toolbar("toolbar-0");
    wait_for_element(&mut d.cx, "preflight".into());
    crate::group_ops::wait_preflight_ready(&mut d.cx);
    let mut events = f.app().subscribe();
    d.press_on("preflight-run");
    crate::group_ops::wait_group_end(&mut d.cx, &mut events);
    if !shown(&mut d.cx, "job-list") {
        d.f6_to("status-jobs");
        d.key("enter");
    }
    wait_for_element(&mut d.cx, "job-list".into());
    d.tab_to("report-toggle-0");
    d.key("down");
    assert!(focused(&mut d.cx, "report-toggle-1"));
    d.key("enter");
    wait_for_element(&mut d.cx, "report-output-1".into());
    d.key("escape");
    assert!(!shown(&mut d.cx, "job-list"));
}

/// Checks that `dialog` has the focus and keeps it on Tab and Shift+Tab.
#[track_caller]
pub(crate) fn keeps_the_focus(cx: &mut VisualTestContext, dialog: &str) {
    assert!(focused(cx, dialog.to_owned()), "{dialog} took no focus");
    for keys in ["tab", "tab", "tab", "tab", "tab", "shift-tab", "shift-tab"] {
        press(cx, keys);
        settle(cx, |_, _| true);
        assert!(
            focused(cx, dialog.to_owned()),
            "{dialog} let the focus out on {keys}"
        );
    }
}

/// Opens `dialog` with `open`, which ends on `opener`, and checks that the
/// dialog takes the focus, keeps Tab and Shift+Tab inside it, closes on
/// Escape and gives the focus back to `opener`.
#[track_caller]
fn dialog_keeps_the_focus(
    d: &mut Driver,
    dialog: &str,
    opener: &str,
    open: impl FnOnce(&mut Driver),
) {
    open(d);
    wait_for_element(&mut d.cx, dialog.to_owned().into());
    keeps_the_focus(&mut d.cx, dialog);
    d.key("escape");
    assert!(!shown(&mut d.cx, dialog), "{dialog} stayed open on Escape");
    assert!(
        focused(&mut d.cx, opener.to_owned()),
        "{dialog} gave the focus to {:?}, not {opener}",
        REGIONS
            .into_iter()
            .find(|region| focused(&mut d.cx, *region)),
    );
}

/// Each dialog takes the focus as it opens, keeps Tab and Shift+Tab inside
/// it, closes on Escape, and gives the focus back to what opened it
/// (M6-25b, M5-10), one row per dialog. Covered elsewhere and not repeated
/// here: the workspace name dialog (`tabs.rs`,
/// `ui_22_dialogs_trap_the_focus_and_close_with_escape`), the palette
/// opened by its key (`palette.rs`), the clone dialog (`clone.rs`), and
/// the job list's rows and output
/// (`ui_22_the_job_list_a_jobs_output_and_its_report_are_reachable_by_keyboard`).
/// The dialogs that open by themselves check the focus in their own tests:
/// git's prompt (`askpass.rs`, `d10_cancelled_askpass_dialog_fails_the_job`),
/// the AI consent (`ai_resolve.rs`,
/// `ai_6_consent_dialog_names_what_is_sent_and_where`) and the missing
/// repos (`clone.rs`,
/// `data_2_import_lists_missing_repos_and_clones_the_checked_ones`).
#[gpui::test]
fn ui_22_every_dialog_takes_keeps_and_gives_back_the_focus(root: &mut TestAppContext) {
    let mut setup = crate::group_ops::Setup::new(&[
        ("api", crate::group_ops::Remote::Ok),
        ("docs", crate::group_ops::Remote::None),
    ]);
    let api = repo(&setup.made, "api");
    api.write("README.md", "changed\n").unwrap();
    setup.add();
    let node = repo_node(&setup.id("api"));
    let f = &setup.fixture;
    let mut d = driver(f, root);
    d.tree_to(&["ungrouped", &node]);
    for (dialog, opener) in [
        ("palette", "toolbar-palette"),
        ("settings-dialog", "toolbar-settings"),
        ("branch-dialog", "toolbar-3"),
        ("operation-dialog", "toolbar-4"),
    ] {
        dialog_keeps_the_focus(&mut d, dialog, opener, |d| d.in_toolbar(opener));
    }
    dialog_keeps_the_focus(&mut d, "job-list", "status-jobs", |d| {
        d.f6_to("status-jobs");
        d.key("enter");
    });
    // On macOS the key marked delete is Backspace (GRP-9: it asks first).
    let delete = if cfg!(target_os = "macos") {
        "backspace"
    } else {
        "delete"
    };
    dialog_keeps_the_focus(&mut d, "discard-dialog", "changes", |d| {
        d.uncommitted(&node);
        d.file_to(List::Unstaged, "README.md");
        d.key(delete);
    });
    dialog_keeps_the_focus(&mut d, "name-dialog", "tree-list", |d| {
        d.tree_to(&["ungrouped"]);
        d.menu("New group…");
    });
    let group = setup.group("Backend", &["api", "docs"]);
    let group_node = setup.group_node(&group);
    wait_for_element(&mut d.cx, tree_row(&group_node));
    dialog_keeps_the_focus(&mut d, "preflight", "toolbar-0", |d| {
        d.tree_to(&[&group_node]);
        d.in_toolbar("toolbar-0");
    });
    for (dialog, opener) in [
        ("group-name", "toolbar-3"),
        ("group-sync", "toolbar-options-0"),
    ] {
        dialog_keeps_the_focus(&mut d, dialog, opener, |d| {
            d.tree_to(&[&group_node]);
            d.in_toolbar(opener);
        });
    }
    dialog_keeps_the_focus(&mut d, "question", "tree-list", |d| {
        d.tree_to(&[&group_node]);
        d.menu("Delete…");
    });

    // The update dialog, with a version on offer.
    let (source, test) = crate::update::offer();
    let f = Fixture::start_with(crate::support::Options {
        updates: Some((source, test)),
        ..crate::support::Options::default()
    });
    let made = crate::history::repos(&["api"]);
    crate::history::add(&f, &made);
    let mut d = driver(&f, root);
    f.app().update_check_now();
    wait_for_element(&mut d.cx, "update-notice".into());
    dialog_keeps_the_focus(&mut d, "update-dialog", "update-notice", |d| {
        d.f6_to("status-jobs");
        d.press_on("update-notice");
    });
}
