//! M6-20: adding folders, cloning, and the missing repos of an import
//! (GRP-1, GRP-2, DATA-2), with real git on `repo-gen` fixtures and bare
//! repos reached over `file://` as remotes (QA-2). The file and folder
//! dialogs are the fixture's fake, answering with a path or a cancel. Jobs
//! are waited for by their `JobEnded`, never by time.

use std::path::{Path, PathBuf};

use gitcroc_app::app::{AppEvent, EVENT_CAPACITY};
use gitcroc_core::{JobView, Outcome, RepoId, WorkspaceId};
use gitcroc_ui::missing_repos::{MissingReposDialog, RowState};
use gitcroc_ui::strings;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext};
use tokio::sync::broadcast::Receiver;

use crate::history::{
    click, label_of, labels_in, press, repo, repos, wait_for_element, wait_until,
};
use crate::support::{Fixture, main_window};

/// A bare repo `name`.git in `dir`, with one commit, and its `file://` URL.
fn bare(dir: &Path, name: &str) -> String {
    let seed = repos(&[name]);
    let target = dir.join(format!("{name}.git"));
    let mut args: Vec<std::ffi::OsString> = vec!["clone".into(), "--bare".into()];
    args.extend(["--quiet".into(), "--".into()]);
    args.extend([seed.path().join(name).into(), target.clone().into()]);
    repo(&seed, name).git(args).unwrap();
    file_url(&target)
}

fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

fn named(id: &str, at: usize) -> ElementId {
    ElementId::NamedInteger(id.to_owned().into(), at as u64)
}

fn secondary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

/// Types `text` where the focus is.
fn input(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}

/// Replaces the text of field `id` with `text`.
fn type_into(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    click(cx, id.into());
    press(cx, &secondary("a"));
    input(cx, text);
}

/// The value of field `id`.
fn value(cx: &mut VisualTestContext, id: ElementId) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.value().map(str::to_owned))
    })
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

fn disabled(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    crate::stage::disabled(cx, id.into())
}

fn focused(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    crate::stage::focused(cx, id.into())
}

/// Opens the add menu beside the filter box.
fn open_add_menu(cx: &mut VisualTestContext) {
    wait_for_element(cx, "tree-add".into());
    click(cx, "tree-add".into());
    wait_for_element(cx, "menu".into());
    assert_eq!(
        label_of(cx, "menu".into()).as_deref(),
        Some(strings::add_repos::ADD_REPOSITORY)
    );
    assert_eq!(
        labels_in(cx, "menu".into(), Role::MenuItem),
        [strings::add_repos::ADD_FOLDER, strings::add_repos::CLONE]
    );
}

fn add_folder(cx: &mut VisualTestContext) {
    open_add_menu(cx);
    click(cx, named("menu-item", 0));
}

fn open_clone(cx: &mut VisualTestContext) {
    open_add_menu(cx);
    click(cx, named("menu-item", 1));
    wait_for_element(cx, "clone-dialog".into());
}

/// Picks `folder` as the clone dialog's parent folder.
fn choose_parent(cx: &mut VisualTestContext, f: &Fixture, folder: &Path) {
    f.answer(Some(folder));
    click(cx, "clone-choose".into());
    let shown = folder.display().to_string();
    wait_until(cx, |cx| {
        label_of(cx, "clone-parent".into()).as_deref() == Some(shown.as_str())
    });
}

/// Waits until the dialog's Clone is enabled.
fn wait_ready(cx: &mut VisualTestContext) {
    wait_until(cx, |cx| !disabled(cx, "dialog-submit"));
}

/// Waits for the next job's end and returns it.
#[track_caller]
fn job_ended(cx: &mut VisualTestContext, events: &mut Receiver<AppEvent>) -> JobView {
    let mut ended = None;
    wait_until(cx, |_| {
        while let Ok(event) = events.try_recv() {
            if let AppEvent::JobEnded(job) = event {
                ended = Some(job);
                return true;
            }
        }
        false
    });
    ended.unwrap()
}

/// The add report's lines.
fn report(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "tree-add-report".into(), Role::ListItem)
}

/// The active workspace's repos, by name.
fn workspace_repos(f: &Fixture) -> Vec<(String, RepoId, PathBuf)> {
    let tabs = f.app().workspace_get();
    let active = tabs.active.clone().unwrap();
    tabs.open
        .into_iter()
        .find(|view| view.id == active)
        .unwrap()
        .repos
        .into_iter()
        .map(|repo| (repo.name, repo.id, PathBuf::from(repo.path)))
        .collect()
}

/// The repo selected in the active tab's tree.
fn selected_repo(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) -> Option<RepoId> {
    cx.update(|_, cx| {
        let tree = window.read(cx).tree(cx)?;
        tree.read(cx).tree().selected_repo().cloned()
    })
}

#[gpui::test]
fn grp_1_add_folder_reports_added_and_already_present(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api", "web"]);
    let empty = tempfile::tempdir().unwrap();
    let (mut cx, _) = main_window(cx, &f);
    // The empty workspace names the add menu's entries.
    wait_for_element(&mut cx, "tree-empty-hint".into());
    assert_eq!(
        label_of(&mut cx, "tree-empty-hint".into()).as_deref(),
        Some(strings::add_repos::EMPTY_HINT)
    );

    let mut events = f.app().subscribe();
    f.answer(Some(made.path()));
    add_folder(&mut cx);
    assert_eq!(
        job_ended(&mut cx, &mut events).outcome,
        Some(Outcome::Succeeded)
    );
    wait_until(&mut cx, |cx| report(cx) == ["Added: api, web"]);

    // GRP-1: the same folder again is a no-op.
    f.answer(Some(made.path()));
    add_folder(&mut cx);
    assert_eq!(
        job_ended(&mut cx, &mut events).outcome,
        Some(Outcome::NothingToDo)
    );
    wait_until(&mut cx, |cx| report(cx) == ["Already present: api, web"]);
    let names: Vec<String> = workspace_repos(&f)
        .into_iter()
        .map(|(name, ..)| name)
        .collect();
    assert_eq!(names, ["api", "web"]);

    // Its own button closes the report.
    click(&mut cx, "tree-add-report-close".into());
    assert!(!shown(&mut cx, "tree-add-report"));

    // A folder with no repo says so: the core refuses it, with why.
    f.answer(Some(empty.path()));
    add_folder(&mut cx);
    assert_eq!(
        job_ended(&mut cx, &mut events).outcome,
        Some(Outcome::Failed)
    );
    wait_until(&mut cx, |cx| {
        let lines = report(cx);
        lines.len() == 1 && lines[0].starts_with("Refused ") && lines[0].contains("no repo found")
    });

    // A cancel adds nothing and shows no report.
    let jobs = f.app().jobs_get().len();
    f.answer(None);
    add_folder(&mut cx);
    crate::support::settle(&mut cx, |_, _| true);
    assert!(!shown(&mut cx, "tree-add-report"));
    assert_eq!(f.app().jobs_get().len(), jobs);
    assert_eq!(workspace_repos(&f).len(), 2);
}

/// ARCH-3, PLAT-2: cloning into the exact Unicode/metacharacter path
/// proves the native dialog forwards an OS path through argument arrays.
#[gpui::test]
fn grp_1_clone_name_follows_the_url_until_edited(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let remotes = tempfile::tempdir().unwrap();
    let url = bare(remotes.path(), "tool");
    let into = tempfile::tempdir().unwrap();
    let into_path = dunce::canonicalize(into.path())
        .unwrap()
        .join("space résumé & $literal ;");
    std::fs::create_dir(&into_path).unwrap();
    let (mut cx, window) = main_window(cx, &f);
    open_clone(&mut cx);
    assert_eq!(
        label_of(&mut cx, "clone-parent".into()).as_deref(),
        Some(strings::clone::NO_FOLDER)
    );
    assert!(disabled(&mut cx, "dialog-submit"));
    choose_parent(&mut cx, &f, &into_path);

    // The core's default: the URL's last path segment without `.git`.
    assert!(focused(&mut cx, "clone-url"));
    input(&mut cx, &url);
    wait_until(&mut cx, |cx| {
        value(cx, "clone-name".into()).as_deref() == Some("tool")
    });
    type_into(&mut cx, "clone-url", "git@example.com:team/lib.git");
    wait_until(&mut cx, |cx| {
        value(cx, "clone-name".into()).as_deref() == Some("lib")
    });

    // Once the user typed a name, the URL leaves it alone.
    type_into(&mut cx, "clone-name", "my-tool");
    type_into(&mut cx, "clone-url", &url);
    wait_ready(&mut cx);
    assert_eq!(
        value(&mut cx, "clone-name".into()).as_deref(),
        Some("my-tool")
    );
    let target = format!(
        "{}{}my-tool",
        into_path.display(),
        std::path::MAIN_SEPARATOR
    );
    assert_eq!(
        label_of(&mut cx, "clone-target".into()),
        Some(strings::clone::clones_into(&target))
    );

    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| !shown(cx, "clone-dialog"));
    let job = job_ended(&mut cx, &mut events);
    assert_eq!(job.outcome, Some(Outcome::Succeeded), "{job:?}");
    assert!(into_path.join("my-tool").join(".git").exists());

    // The clone is in the workspace and selected in the tree.
    let cloned = workspace_repos(&f);
    assert_eq!(cloned.len(), 1);
    let id = cloned[0].1.clone();
    wait_until(&mut cx, |cx| {
        selected_repo(cx, &window).as_ref() == Some(&id)
    });

    // The parent folder is kept for the next clone.
    open_clone(&mut cx);
    assert_eq!(
        label_of(&mut cx, "clone-parent".into()),
        Some(into_path.display().to_string())
    );
}

#[gpui::test]
fn grp_1_clone_refuses_a_folder_that_is_not_empty(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let remotes = tempfile::tempdir().unwrap();
    let url = bare(remotes.path(), "tool");
    let into = tempfile::tempdir().unwrap();
    let into_path = dunce::canonicalize(into.path()).unwrap();
    std::fs::create_dir(into_path.join("tool")).unwrap();
    std::fs::write(into_path.join("tool").join("kept.txt"), "kept\n").unwrap();
    let (mut cx, _) = main_window(cx, &f);
    open_clone(&mut cx);
    choose_parent(&mut cx, &f, &into_path);
    input(&mut cx, &url);
    wait_until(&mut cx, |cx| shown(cx, "clone-target-error"));
    let why = label_of(&mut cx, "clone-target-error".into()).unwrap();
    assert!(why.contains("exists and is not empty"), "{why}");
    assert!(disabled(&mut cx, "dialog-submit"));

    // Neither the button nor Enter starts a job.
    let jobs = f.app().jobs_get().len();
    click(&mut cx, "dialog-submit".into());
    press(&mut cx, "enter");
    crate::support::settle(&mut cx, |_, _| true);
    assert!(shown(&mut cx, "clone-dialog"));
    assert_eq!(f.app().jobs_get().len(), jobs);
    assert!(workspace_repos(&f).is_empty());

    // Another name clears the refusal.
    type_into(&mut cx, "clone-name", "tool-2");
    wait_ready(&mut cx);
    assert!(!shown(&mut cx, "clone-target-error"));
}

#[gpui::test]
fn grp_2_clone_adds_the_repo_to_an_existing_or_a_new_group(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let remotes = tempfile::tempdir().unwrap();
    let api = bare(remotes.path(), "api");
    let web = bare(remotes.path(), "web");
    let into = tempfile::tempdir().unwrap();
    let into_path = dunce::canonicalize(into.path()).unwrap();
    let backend = crate::history_merged::group(&f, "Backend", Vec::new());
    let (mut cx, _) = main_window(cx, &f);

    // Into the existing group.
    open_clone(&mut cx);
    choose_parent(&mut cx, &f, &into_path);
    input(&mut cx, &api);
    assert_eq!(
        label_of(&mut cx, named("clone-group", 0)).as_deref(),
        Some("Backend")
    );
    click(&mut cx, named("clone-group", 0));
    wait_ready(&mut cx);
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    let job = job_ended(&mut cx, &mut events);
    assert_eq!(job.outcome, Some(Outcome::Succeeded), "{job:?}");
    let groups = |f: &Fixture| {
        let tabs = f.app().workspace_get();
        tabs.open[0].groups.clone()
    };
    let api_id = workspace_repos(&f)[0].1.clone();
    wait_until(&mut cx, |_| {
        groups(&f)
            .iter()
            .any(|group| group.id == backend && group.repos == [api_id.clone()])
    });

    // A new group whose name is taken is refused inside the open dialog.
    wait_until(&mut cx, |cx| !shown(cx, "clone-dialog"));
    open_clone(&mut cx);
    input(&mut cx, &web);
    click(&mut cx, "clone-group-new".into());
    assert!(focused(&mut cx, "clone-group-name"));
    input(&mut cx, "backend");
    wait_ready(&mut cx);
    let jobs = f.app().jobs_get().len();
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| shown(cx, "dialog-error"));
    assert!(shown(&mut cx, "clone-dialog"));
    assert_eq!(f.app().jobs_get().len(), jobs);

    // A free name makes the group, with the repo alone.
    type_into(&mut cx, "clone-group-name", "Frontend");
    assert!(!shown(&mut cx, "dialog-error"));
    wait_ready(&mut cx);
    click(&mut cx, "dialog-submit".into());
    let job = job_ended(&mut cx, &mut events);
    assert_eq!(job.outcome, Some(Outcome::Succeeded), "{job:?}");
    wait_until(&mut cx, |_| {
        groups(&f).iter().any(|group| {
            group.name == "Frontend" && group.repos.len() == 1 && group.repos[0] != api_id
        })
    });
}

#[gpui::test]
fn ui_22_the_add_menu_and_clone_dialog_work_from_the_keyboard(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let remotes = tempfile::tempdir().unwrap();
    let url = bare(remotes.path(), "tool");
    let into = tempfile::tempdir().unwrap();
    let into_path = dunce::canonicalize(into.path()).unwrap();
    let (mut cx, _) = main_window(cx, &f);

    // From the filter box, Tab reaches the add button; Enter opens its
    // menu with the first entry focused, and Escape gives the focus back.
    wait_for_element(&mut cx, "tree-add".into());
    click(&mut cx, "tree-filter".into());
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "tree-add"));
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "menu".into());
    assert!(focused(&mut cx, named("menu-item", 0)));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "menu"));
    assert!(focused(&mut cx, "tree-add"));

    // Down and Enter choose Clone…; the URL has the focus, and Escape
    // closes the dialog.
    press(&mut cx, "space");
    wait_for_element(&mut cx, "menu".into());
    press(&mut cx, "down");
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "clone-dialog".into());
    assert!(focused(&mut cx, "clone-url"));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "clone-dialog"));
    assert!(focused(&mut cx, "tree-add"));

    // The whole clone, without the mouse.
    press(&mut cx, "enter");
    press(&mut cx, "down");
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "clone-dialog".into());
    input(&mut cx, &url);
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "clone-choose"));
    f.answer(Some(&into_path));
    press(&mut cx, "enter");
    wait_until(&mut cx, |cx| {
        label_of(cx, "clone-parent".into()) == Some(into_path.display().to_string())
    });
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "clone-name"));
    wait_ready(&mut cx);
    let mut events = f.app().subscribe();
    press(&mut cx, "enter");
    wait_until(&mut cx, |cx| !shown(cx, "clone-dialog"));
    let job = job_ended(&mut cx, &mut events);
    assert_eq!(job.outcome, Some(Outcome::Succeeded), "{job:?}");
    assert!(into_path.join("tool").join(".git").exists());
}

/// A workspace file `name` in `dir` with `repos` as (id, path, remote).
fn workspace_file(dir: &Path, name: &str, repos: &[(&str, &str, Option<&str>)]) -> PathBuf {
    let quoted = |text: &str| toml::Value::String(text.to_owned()).to_string();
    let mut text = format!("schema = 1\nname = {}\n", quoted(name));
    for (id, path, remote) in repos {
        text.push_str(&format!(
            "\n[[repo]]\nid = {}\npath = {}\n",
            quoted(id),
            quoted(path)
        ));
        if let Some(remote) = remote {
            text.push_str(&format!("remote = {}\n", quoted(remote)));
        }
    }
    let file = dir.join("shared.toml");
    std::fs::write(&file, text).unwrap();
    file
}

/// Chooses Import workspace… in "+"'s menu.
fn import(cx: &mut VisualTestContext, f: &Fixture, file: &Path) {
    f.answer(Some(file));
    click(cx, "tab-add".into());
    wait_for_element(cx, "menu".into());
    let at = labels_in(cx, "menu".into(), Role::MenuItem)
        .iter()
        .position(|label| label == strings::workspaces::IMPORT_ENTRY)
        .unwrap();
    click(cx, named("menu-item", at));
}

fn missing_rows(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
) -> Vec<(RepoId, bool, RowState)> {
    cx.update(|_, cx| {
        window
            .read(cx)
            .dialog_of::<MissingReposDialog>()
            .map(|dialog| dialog.read(cx).rows())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn data_2_import_lists_missing_repos_and_clones_the_checked_ones(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let remotes = tempfile::tempdir().unwrap();
    let api = bare(remotes.path(), "api");
    let gone = file_url(&remotes.path().join("gone.git"));
    let shared = tempfile::tempdir().unwrap();
    let shared_path = dunce::canonicalize(shared.path()).unwrap();
    let file = workspace_file(
        &shared_path,
        "Shared",
        &[
            ("r1", "api", Some(&api)),
            ("r2", "web", None),
            ("r3", "nested/bad", Some(&gone)),
        ],
    );
    let (mut cx, window) = main_window(cx, &f);
    import(&mut cx, &f, &file);

    // The import opens the new workspace, then the dialog with its
    // missing repos.
    wait_for_element(&mut cx, "missing-dialog".into());
    wait_until(&mut cx, |cx| missing_rows(cx, &window).len() == 3);
    // It takes the focus and keeps it (M6-25b).
    crate::keyboard::keeps_the_focus(&mut cx, "missing-dialog");
    assert_eq!(
        f.app().workspace_get().open.len(),
        2,
        "the import opened a tab"
    );
    let rows = missing_rows(&mut cx, &window);
    assert_eq!(
        rows.iter()
            .map(|(_, checked, _)| *checked)
            .collect::<Vec<_>>(),
        [true, false, true]
    );
    // A repo with no remote can't be checked, and says so.
    assert!(disabled(&mut cx, named("missing-check", 1)));
    assert_eq!(
        label_of(&mut cx, named("missing-remote", 1)).as_deref(),
        Some(strings::missing_repos::NO_REMOTE)
    );
    assert_eq!(
        label_of(&mut cx, named("missing-remote", 0)).as_deref(),
        Some(api.as_str())
    );
    assert_eq!(
        label_of(&mut cx, named("missing-parent", 0)),
        Some(shared_path.display().to_string())
    );
    assert_eq!(
        value(&mut cx, named("missing-folder", 2)).as_deref(),
        Some("bad")
    );

    // Clone selected clones the checked ones, all at once, into their
    // rows' targets; the parent of a nested one is created.
    wait_ready(&mut cx);
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    let mut outcomes = vec![
        job_ended(&mut cx, &mut events).outcome,
        job_ended(&mut cx, &mut events).outcome,
    ];
    outcomes.sort_by_key(|outcome| format!("{outcome:?}"));
    assert_eq!(outcomes, [Some(Outcome::Failed), Some(Outcome::Succeeded)]);
    wait_until(&mut cx, |cx| {
        let states: Vec<RowState> = missing_rows(cx, &window)
            .into_iter()
            .map(|(_, _, state)| state)
            .collect();
        states == [RowState::Cloned, RowState::Idle, RowState::Failed]
    });
    assert_eq!(
        label_of(&mut cx, named("missing-state", 0)).as_deref(),
        Some(strings::missing_repos::CLONED)
    );
    // A failing URL shows git's message on its row.
    let failed = label_of(&mut cx, named("missing-state", 2)).unwrap();
    assert!(failed.starts_with("Failed: "), "{failed}");
    assert!(failed.len() > "Failed: ".len() + 10, "{failed}");
    assert!(shared_path.join("api").join(".git").exists());
    assert!(!shared_path.join("web").exists());
    // The dialog stays open until closed.
    assert!(shown(&mut cx, "missing-dialog"));
    click(&mut cx, "dialog-cancel".into());
    assert!(!shown(&mut cx, "missing-dialog"));
}

#[gpui::test]
fn data_2_a_missing_repos_menu_opens_the_dialog_with_only_it_checked(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let remotes = tempfile::tempdir().unwrap();
    let made = repos(&["api", "web"]);
    for name in ["api", "web"] {
        let url = bare(remotes.path(), name);
        repo(&made, name)
            .git(["remote", "add", "origin", url.as_str()])
            .unwrap();
    }
    let added = crate::history::add(&f, &made);
    for name in ["api", "web"] {
        std::fs::remove_dir_all(made.path().join(name)).unwrap();
    }
    f.refresh(&f.active());
    let api = crate::history::id_of(&added, "api");
    let (mut cx, window) = main_window(cx, &f);
    let row = crate::history::tree_row(&format!("repo:{}", api.0));
    wait_for_element(
        &mut cx,
        crate::history::tree_part(&format!("repo:{}", api.0), "missing"),
    );
    cx.update(|window, cx| window.right_click(row, cx));
    cx.run_until_parked();
    wait_for_element(&mut cx, "menu".into());
    let item = named("menu-item", 0);
    assert_eq!(
        label_of(&mut cx, item.clone()).as_deref(),
        Some(strings::tree_menu::CLONE_FROM_REMOTE)
    );
    assert!(!disabled(&mut cx, item.clone()));
    click(&mut cx, item);
    wait_for_element(&mut cx, "missing-dialog".into());
    wait_until(&mut cx, |cx| missing_rows(cx, &window).len() == 2);
    let checked: Vec<(RepoId, bool)> = missing_rows(&mut cx, &window)
        .into_iter()
        .map(|(id, checked, _)| (id, checked))
        .collect();
    let web = crate::history::id_of(&added, "web");
    assert_eq!(checked, [(api.clone(), true), (web, false)]);

    // Its clone fills the repo in where it was.
    wait_ready(&mut cx);
    let mut events = f.app().subscribe();
    click(&mut cx, "dialog-submit".into());
    let job = job_ended(&mut cx, &mut events);
    assert_eq!(job.outcome, Some(Outcome::Succeeded), "{job:?}");
    wait_until(&mut cx, |cx| {
        missing_rows(cx, &window)
            .first()
            .is_some_and(|(_, _, state)| *state == RowState::Cloned)
    });
    assert!(made.path().join("api").join(".git").exists());
    assert!(!made.path().join("web").exists());
    assert_eq!(workspace_repos(&f).len(), 2);
}

#[gpui::test]
fn data_2_export_and_import_notices(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(&["api"]);
    crate::history::add(&f, &made);
    let out = tempfile::tempdir().unwrap();
    let out_path = dunce::canonicalize(out.path()).unwrap();
    let (mut cx, _) = main_window(cx, &f);

    // The tab's menu exports; the notice names the file.
    let tab = crate::history::tab(&f.active());
    wait_for_element(&mut cx, tab.clone());
    f.answer(Some(&out_path.join("team.toml")));
    cx.update(|window, cx| window.right_click(tab, cx));
    cx.run_until_parked();
    let at = labels_in(&mut cx, "menu".into(), Role::MenuItem)
        .iter()
        .position(|label| label == strings::tabs::EXPORT)
        .unwrap();
    click(&mut cx, named("menu-item", at));
    let written = out_path.join("team.toml").display().to_string();
    wait_until(&mut cx, |cx| shown(cx, "share-notice"));
    assert_eq!(
        label_of(&mut cx, "share-notice".into()),
        Some(strings::share::exported(&written, &[]))
    );
    assert!(out_path.join("team.toml").exists());

    // Paths no relative path reaches are named, as the core reports them.
    let heard = gitcroc_ui::share::hear(&AppEvent::WorkspaceExported {
        workspace_id: WorkspaceId("w".into()),
        report: Some(gitcroc_core::ExportReport {
            path: written.clone(),
            absolute: vec![r"D:\src\api".to_owned()],
        }),
        error: None,
    })
    .unwrap();
    let notice = heard.notice.unwrap();
    assert!(notice.error);
    assert_eq!(
        notice.text.as_ref(),
        format!(
            "Exported the workspace to {written}. No relative path reaches these repos, so they stay absolute: D:\\src\\api."
        )
    );

    // A refused import says why and opens no tab.
    let bad = out_path.join("newer.toml");
    std::fs::write(&bad, "schema = 99\nname = \"Later\"\n").unwrap();
    import(&mut cx, &f, &bad);
    wait_until(&mut cx, |cx| {
        label_of(cx, "share-notice".into()).is_some_and(|text| !text.starts_with("Exported"))
    });
    let why = label_of(&mut cx, "share-notice".into()).unwrap();
    assert!(why.chars().next().unwrap().is_uppercase(), "{why}");
    assert_eq!(f.app().workspace_get().open.len(), 1);
    assert!(!shown(&mut cx, "missing-dialog"));
    click(&mut cx, "share-notice-dismiss".into());
    assert!(!shown(&mut cx, "share-notice"));
}

#[gpui::test]
fn sec_3_clone_and_missing_repo_text_renders_as_text(cx: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let markup_url = "https://example.com/<b>team</b>/&amp;{x}.git";
    let into = tempfile::tempdir().unwrap();
    let parent = dunce::canonicalize(into.path()).unwrap().join("&amp; {x}");
    std::fs::create_dir(&parent).unwrap();
    let (mut cx, window) = main_window(cx, &f);
    open_clone(&mut cx);
    choose_parent(&mut cx, &f, &parent);
    input(&mut cx, markup_url);
    assert_eq!(
        value(&mut cx, "clone-url".into()).as_deref(),
        Some(markup_url)
    );
    wait_until(&mut cx, |cx| {
        value(cx, "clone-name".into()).as_deref() == Some("&amp;{x}")
    });
    let target = format!(
        "{}{}&amp;{{x}}",
        parent.display(),
        std::path::MAIN_SEPARATOR
    );
    wait_until(&mut cx, |cx| {
        label_of(cx, "clone-target".into()) == Some(strings::clone::clones_into(&target))
    });
    press(&mut cx, "escape");

    // A shared file's names and URLs show as written.
    let shared = tempfile::tempdir().unwrap();
    let shared_path = dunce::canonicalize(shared.path()).unwrap();
    let remote = "<i>git@example.com</i>:&lt;team&gt;/api.git";
    let file = workspace_file(
        &shared_path,
        "<b>Shared</b>",
        &[("r1", "<b>api&amp;{x}", Some(remote))],
    );
    import(&mut cx, &f, &file);
    wait_for_element(&mut cx, "missing-dialog".into());
    wait_until(&mut cx, |cx| missing_rows(cx, &window).len() == 1);
    assert_eq!(
        label_of(&mut cx, named("missing-row", 0)).as_deref(),
        Some("<b>api&amp;{x}")
    );
    assert_eq!(
        label_of(&mut cx, named("missing-remote", 0)).as_deref(),
        Some(remote)
    );
    assert_eq!(
        value(&mut cx, named("missing-folder", 0)).as_deref(),
        Some("<b>api&amp;{x}")
    );
}
