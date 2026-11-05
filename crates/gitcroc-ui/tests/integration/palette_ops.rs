//! M6-24a: every operation of the palette's catalog does what its own
//! control does (UI-22), each from a state that gives its subject.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::history::{
    click, label_of, labels_in, repo, tree_part, tree_row, wait_for_element, wait_until,
};
use crate::palette::{open_palette, search};
use crate::support::{Fixture, main_window, settle};
use gitcroc_app::platform::{FileManager, Launch, Terminal};
use gitcroc_core::{JobState, PaletteOp, RepoId, palette_catalog};
use gitcroc_ui::changes::List;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext};

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

/// What choosing an operation is to do, as its control does.
pub(crate) enum Did {
    /// The dialog with this id opens.
    Dialog(&'static str),
    /// A job starts, or the dialog that runs it asks to confirm first.
    Runs,
    /// Something else, which holds once done.
    Check(Box<dyn Fn(&mut VisualTestContext) -> bool>),
}

pub(crate) struct Driver<'a> {
    pub(crate) f: &'a Fixture,
    pub(crate) cx: VisualTestContext,
    pub(crate) window: Entity<GitcrocWindow>,
    pub(crate) done: BTreeSet<String>,
}

pub(crate) fn label(op: PaletteOp) -> &'static str {
    palette_catalog()
        .iter()
        .find(|entry| entry.op == op)
        .map(|entry| entry.label)
        .unwrap()
}

fn reason(cx: &mut VisualTestContext) -> Option<String> {
    label_of(cx, ElementId::Name("palette-row-0-reason".into()))
}

pub(crate) fn shown(cx: &mut VisualTestContext, id: &str) -> bool {
    let id: ElementId = id.to_owned().into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    crate::history::press(cx, keys);
}

impl Driver<'_> {
    pub(crate) fn jobs(&self) -> usize {
        self.f.app().jobs_get().len()
    }

    /// Waits until every job ended, so the next step starts from them.
    pub(crate) fn settled(&mut self) {
        let f = self.f;
        wait_until(&mut self.cx, |_| {
            f.app()
                .jobs_get()
                .iter()
                .all(|job| job.state == JobState::Ended)
        });
    }

    /// Closes the dialogs over the window.
    pub(crate) fn close_dialogs(&mut self) {
        for _ in 0..5 {
            if !shown(&mut self.cx, "dialog-layer") {
                return;
            }
            press(&mut self.cx, "escape");
        }
        assert!(!shown(&mut self.cx, "dialog-layer"), "a dialog stays open");
    }

    /// Selects a repo, group or branch by its palette row.
    fn select(&mut self, name: &str) {
        let (cx, window) = (&mut self.cx, &self.window);
        open_palette(cx, window);
        search(cx, window, name, name);
        press(cx, "enter");
        assert!(!shown(cx, "palette"));
        settle(cx, |_, _| true);
    }

    /// Clicks tree row `node`, expanding the nodes `above` it first.
    fn select_node(&mut self, above: &[&str], node: &str) {
        let cx = &mut self.cx;
        for parent in above {
            wait_for_element(cx, tree_row(parent));
            let expanded = cx.update(|window, cx| {
                window.render_frame(cx);
                window.find(tree_row(parent)).expanded()
            });
            if expanded != Some(true) {
                click(cx, tree_part(parent, "chevron"));
            }
        }
        wait_for_element(cx, tree_row(node));
        click(cx, tree_row(node));
    }

    /// Chooses `op` in the palette once it can apply, and checks that it
    /// did what `did` says.
    fn choose(&mut self, op: PaletteOp, did: Did) {
        let jobs = self.jobs();
        let name = label(op);
        let (cx, window) = (&mut self.cx, &self.window);
        open_palette(cx, window);
        search(cx, window, name, name);
        for _ in 0..500 {
            settle(cx, |_, _| true);
            if reason(cx).is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(reason(cx), None, "{name} cannot apply");
        press(cx, "enter");
        assert!(!shown(cx, "palette"), "{name} left the palette open");
        self.expect(name, jobs, did);
    }

    /// Checks that operation `name` did what `did` says, `jobs` being how
    /// many jobs there were before.
    pub(crate) fn expect(&mut self, name: &str, jobs: usize, did: Did) {
        let f = self.f;
        match did {
            Did::Dialog(id) => {
                wait_for_element(&mut self.cx, id.to_owned().into());
                self.close_dialogs();
            }
            Did::Runs => {
                let ran = |cx: &mut VisualTestContext| {
                    f.app().jobs_get().len() > jobs
                        || shown(cx, "op-confirm-actions")
                        || shown(cx, "branch-confirm")
                };
                for _ in 0..1000 {
                    settle(&mut self.cx, |_, _| true);
                    if ran(&mut self.cx) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                if !ran(&mut self.cx) {
                    let dialog = labels_in(&mut self.cx, "dialog-layer".into(), Role::Dialog);
                    let buttons = labels_in(&mut self.cx, "dialog-layer".into(), Role::Button);
                    panic!(
                        "{name} started no job and asked nothing: {jobs} jobs before, {} now; \
                         dialogs {dialog:?} with buttons {buttons:?}",
                        f.app().jobs_get().len()
                    );
                }
                self.close_dialogs();
                self.settled();
            }
            Did::Check(check) => wait_until(&mut self.cx, |cx| check(cx)),
        }
        self.done.insert(name.to_owned());
    }
}

fn repo_node(id: &RepoId) -> String {
    format!("repo:{}", id.0)
}

#[gpui::test]
fn ui_22_every_palette_operation_runs_its_action(root: &mut TestAppContext) {
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
        setup.add();
        let id = setup.id("api");
        let f = &setup.fixture;
        let (mut cx, window) = main_window(root, f);
        crate::stage::enlarge(&mut cx);
        let mut d = Driver {
            f,
            cx,
            window,
            done: BTreeSet::new(),
        };
        d.select("api");
        // The toolbar's and the window's.
        for op in [Op::Fetch, Op::Pull, Op::Push] {
            d.choose(op, Did::Runs);
        }
        for op in [
            Op::FetchWithOptions,
            Op::PullWithOptions,
            Op::PushWithOptions,
            Op::Merge,
            Op::Rebase,
            Op::Stash,
            Op::CreateTag,
        ] {
            d.choose(op, Did::Dialog("operation-dialog"));
        }
        d.choose(Op::CreateBranch, Did::Dialog("branch-dialog"));
        d.choose(Op::Settings, Did::Dialog("settings-dialog"));
        d.choose(Op::NewWorkspace, Did::Dialog("name-dialog"));
        d.choose(Op::NewGroup, Did::Dialog("name-dialog"));
        d.choose(Op::JobList, Did::Dialog("job-list"));
        d.choose(Op::Clone, Did::Dialog("clone-dialog"));
        d.choose(
            Op::OpenInFileManager,
            Did::Check(Box::new(|_| FILES.load(Ordering::SeqCst) > 0)),
        );
        d.choose(
            Op::OpenInTerminal,
            Did::Check(Box::new(|_| TERMS.load(Ordering::SeqCst) > 0)),
        );
        // The history's commit menu.
        crate::history::wait_for_element(&mut d.cx, "history-row-0".into());
        click(&mut d.cx, "history-row-0".into());
        for op in [Op::ResetSoft, Op::ResetMixed, Op::ResetHard] {
            d.choose(op, Did::Dialog("operation-dialog"));
        }
        d.choose(Op::Revert, Did::Runs);
        click(&mut d.cx, "history-row-1".into());
        d.choose(Op::CherryPick, Did::Runs);
        // Reapplying this commit may conflict, depending on which history
        // page the preceding revert has delivered. End that operation and
        // wait for the UI's unblocked summary before testing checkout.
        d.abort_cherry_pick(&api, &id);
        click(&mut d.cx, "history-row-1".into());
        d.choose(Op::CheckoutCommit, Did::Runs);
        api.git(["checkout", "--quiet", "main"]).unwrap();
        // The tree's rows.
        let node = repo_node(&id);
        let tags = format!("{node}:tags");
        d.select_node(&[&node, &tags], &format!("{tags}:ref:v1"));
        d.choose(Op::DeleteTag, Did::Dialog("operation-dialog"));
        d.choose(Op::PushTag, Did::Runs);
        let stashes = format!("{node}:stashes");
        let stash = format!("{stashes}:ref:0");
        d.select_node(&[&node, &stashes], &stash);
        d.choose(Op::StashApply, Did::Runs);
        d.select_node(&[&node, &stashes], &stash);
        d.choose(Op::StashDrop, Did::Runs);
        d.select_node(&[&node, &stashes], &stash);
        d.choose(Op::StashPop, Did::Runs);
        d.select("topic");
        d.choose(Op::RenameBranch, Did::Dialog("branch-dialog"));
        d.choose(Op::DeleteBranch, Did::Dialog("branch-dialog"));
        d.choose(Op::SetUpstream, Did::Dialog("branch-dialog"));
        d.choose(Op::Checkout, Did::Runs);
        d.select("origin/main");
        d.choose(Op::CheckoutRemoteBranch, Did::Runs);
        let remotes = format!("{node}:remotes");
        d.select_node(&[&node], &remotes);
        d.choose(Op::AddRemote, Did::Dialog("operation-dialog"));
        let origin = format!("{remotes}:origin");
        d.select_node(&[&node, &remotes], &origin);
        d.choose(Op::RenameRemote, Did::Dialog("operation-dialog"));
        d.choose(Op::EditRemoteUrl, Did::Dialog("operation-dialog"));
        d.choose(Op::RemoveRemote, Did::Runs);
        // The tabs, and the workspace's file.
        let first = f.active();
        let app = f.app().clone();
        f.runtime
            .block_on(app.workspace_create("Second".to_owned()))
            .unwrap();
        let w = d.window.clone();
        wait_until(&mut d.cx, |cx| {
            cx.update(|_, cx| w.read(cx).tabs(cx).open.len()) == 2
        });
        let active = |cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>| {
            cx.update(|_, cx| window.read(cx).tabs(cx).active)
        };
        let second = active(&mut d.cx, &d.window).unwrap();
        assert_ne!(second, first);
        // The tab bar has the focus, as after a click on the new tab.
        crate::changes::switch_to(&mut d.cx, f, &d.window.clone(), &second);
        let w = d.window.clone();
        let back = first.clone();
        d.choose(
            Op::PreviousTab,
            Did::Check(Box::new(move |cx| active(cx, &w).as_ref() == Some(&back))),
        );
        let w = d.window.clone();
        let ahead = second.clone();
        d.choose(
            Op::NextTab,
            Did::Check(Box::new(move |cx| active(cx, &w).as_ref() == Some(&ahead))),
        );
        let file = setup.made.path().join("second.gitcroc.json");
        f.answer(Some(&file));
        let written = file.clone();
        d.choose(
            Op::ExportWorkspace,
            Did::Check(Box::new(move |_| written.exists())),
        );
        let w = d.window.clone();
        d.choose(
            Op::CloseTab,
            Did::Check(Box::new(move |cx| {
                cx.update(|_, cx| w.read(cx).tabs(cx).open.len()) == 1
            })),
        );
        f.answer(Some(&file));
        let w = d.window.clone();
        d.choose(
            Op::ImportWorkspace,
            Did::Check(Box::new(move |cx| {
                cx.update(|_, cx| w.read(cx).tabs(cx).open.len()) == 2
            })),
        );
        crate::changes::switch_to(&mut d.cx, f, &d.window.clone(), &first);
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
        d.choose(
            Op::AddFolder,
            Did::Check(Box::new(move |_| grown() > before)),
        );
        d.select("web");
        let after = names();
        d.choose(
            Op::RemoveRepo,
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
        let (mut cx, window) = main_window(root, &f);
        crate::stage::show_uncommitted(&mut cx, &window, &added, "api");
        crate::stage::wait_for_lists(&mut cx, &window, &[], &["README.md", "new.txt"]);
        let mut d = Driver {
            f: &f,
            cx,
            window,
            done: BTreeSet::new(),
        };
        let row = |d: &mut Driver, list: List, path: &str| {
            let at = crate::stage::row_of(&mut d.cx, &d.window, list, path);
            click(&mut d.cx, crate::changes::row(at));
        };
        row(&mut d, List::Unstaged, "new.txt");
        d.choose(Op::StageFile, Did::Runs);
        crate::stage::wait_for_lists(&mut d.cx, &d.window, &["new.txt"], &["README.md"]);
        row(&mut d, List::Staged, "new.txt");
        d.choose(Op::UnstageFile, Did::Runs);
        crate::stage::wait_for_lists(&mut d.cx, &d.window, &[], &["README.md", "new.txt"]);
        row(&mut d, List::Unstaged, "README.md");
        d.choose(Op::DiscardFile, Did::Dialog("discard-dialog"));
        d.choose(Op::StageAll, Did::Runs);
        crate::stage::wait_for_lists(&mut d.cx, &d.window, &["README.md", "new.txt"], &[]);
        d.choose(Op::UnstageAll, Did::Runs);
        crate::stage::wait_for_lists(&mut d.cx, &d.window, &[], &["README.md", "new.txt"]);
        d.choose(Op::StageAll, Did::Runs);
        crate::stage::wait_for_lists(&mut d.cx, &d.window, &["README.md", "new.txt"], &[]);
        // A change left out keeps the changes, and the commit box, shown.
        api.write("left.txt", "left\n").unwrap();
        crate::stage::wait_for_lists(
            &mut d.cx,
            &d.window,
            &["README.md", "new.txt"],
            &["left.txt"],
        );
        click(&mut d.cx, "commit-subject".into());
        d.cx.update(|window, cx| window.input("From the palette", cx));
        d.cx.run_until_parked();
        d.choose(Op::Commit, Did::Runs);
        // Amend first fills in the last message, as its button does.
        let w = d.window.clone();
        d.choose(
            Op::Amend,
            Did::Check(Box::new(move |cx| {
                cx.update(|_, cx| {
                    let changes = w.read(cx).changes(cx).unwrap();
                    let commit_box = changes.read(cx).commit_box().read(cx);
                    commit_box
                        .draft()
                        .is_some_and(|draft| draft.subject == "From the palette")
                })
            })),
        );
        done.extend(d.done);
    }

    // The conflict editor, the banner and AI.
    {
        use crate::ai_resolve::{Open, mock_provider, open, resolving, start};
        let f = start(crate::support::AiScript {
            replies: vec![resolving(&[(1, "both\n")]), resolving(&[(1, "both\n")])],
            ..crate::support::AiScript::default()
        });
        mock_provider(&f, "Mock", true);
        let Open {
            f,
            cx,
            window,
            _made,
            r,
        } = open(root, f, &["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]);
        let mut d = Driver {
            f: &f,
            cx,
            window,
            done: BTreeSet::new(),
        };
        for (path, op) in [
            ("a.txt", Op::TakeOurs),
            ("b.txt", Op::TakeTheirs),
            ("c.txt", Op::TakeBoth),
        ] {
            crate::ai_resolve::row_click(&mut d.cx, &d.window, path);
            let editor = crate::conflict_flow::wait_for_editor(&mut d.cx, &d.window, path);
            let before = crate::conflict_flow::value(&mut d.cx, &editor);
            // The caret goes into the hunk, as F7 takes it there.
            d.cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.navigate(true, window, cx))
            });
            d.cx.run_until_parked();
            d.choose(
                op,
                Did::Check(Box::new(move |cx| {
                    crate::conflict_flow::value(cx, &editor) != before
                })),
            );
        }
        crate::ai_resolve::row_click(&mut d.cx, &d.window, "a.txt");
        crate::conflict_flow::wait_for_editor(&mut d.cx, &d.window, "a.txt");
        d.choose(Op::MarkResolved, Did::Runs);
        // The job can end before its new list reaches the pane. Selecting
        // d.txt by the old row index would select a different file.
        wait_until(&mut d.cx, |cx| {
            crate::changes::with_changes(cx, &d.window, |changes| {
                !changes.loading()
                    && changes.rows().iter().all(|row| {
                        !matches!(row, gitcroc_ui::changes::FileRow::File { file, .. }
                        if file.list == List::Unmerged
                            && changes.entry_of(file).is_some_and(|entry| entry.path() == "a.txt"))
                    })
            })
        });
        crate::ai_resolve::row_click(&mut d.cx, &d.window, "d.txt");
        let editor = crate::conflict_flow::wait_for_editor(&mut d.cx, &d.window, "d.txt");
        d.choose(Op::ResolveWithAi, Did::Runs);
        let proposed = editor.clone();
        wait_until(&mut d.cx, |cx| {
            crate::ai_resolve::proposed(cx, &proposed) > 0
        });
        d.choose(
            Op::AcceptHighConfidence,
            Did::Check(Box::new(move |cx| {
                !crate::conflict_flow::value(cx, &editor).contains("<<<<<<<")
            })),
        );
        d.choose(Op::ResolveAllWithAi, Did::Runs);
        d.settled();
        d.choose(Op::Abort, Did::Runs);
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
        let (mut cx, window) = main_window(root, &f);
        crate::stage::enlarge(&mut cx);
        crate::changes::show_repo(&mut cx, &window, &added, "api");
        wait_for_element(&mut cx, "conflict-banner".into());
        let mut d = Driver {
            f: &f,
            cx,
            window,
            done: BTreeSet::new(),
        };
        d.choose(Op::Continue, Did::Runs);
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
        let (mut cx, window) = main_window(root, &f);
        crate::stage::show_uncommitted(&mut cx, &window, &added, "api");
        let mut d = Driver {
            f: &f,
            cx,
            window,
            done: BTreeSet::new(),
        };
        for (path, op) in [("f.bin", Op::KeepOurs), ("g.bin", Op::KeepTheirs)] {
            crate::ai_resolve::row_click(&mut d.cx, &d.window, path);
            wait_for_element(&mut d.cx, "conflict-use-ours".into());
            d.choose(op, Did::Runs);
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
        let added = crate::history::add(&f, &made);
        let (mut cx, window) = main_window(root, &f);
        crate::stage::enlarge(&mut cx);
        crate::changes::show_repo(&mut cx, &window, &added, "api");
        f.app().update_check_now();
        wait_for_element(&mut cx, "update-notice".into());
        let mut d = Driver {
            f: &f,
            cx,
            window,
            done: BTreeSet::new(),
        };
        d.choose(Op::CheckForUpdates, Did::Dialog("update-dialog"));
        done.extend(d.done);
    }

    let all: BTreeSet<String> = palette_catalog()
        .iter()
        .map(|entry| entry.label.to_owned())
        .collect();
    let missing: Vec<&String> = all.difference(&done).collect();
    assert!(missing.is_empty(), "not run from the palette: {missing:?}");
}
