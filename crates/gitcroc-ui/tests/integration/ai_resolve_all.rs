//! M6-23c: Resolve all with AI from the conflict banner (AI-3, AI-6,
//! AI-13, AI-16, D9, SEC-3, UI-22, UI-30), the port of
//! rc.1's `ui/src/conflict/resolveAll.test.tsx`. Real git conflicts with several
//! unmerged files, one of them on the exclude list (QA-2); the provider is
//! the fixture's mock, held where a test needs files still running and
//! released by the test, never waited for by time.

use gitcroc_core::{Outcome, WorkspaceId};
use gitcroc_ui::conflict::{ConflictEditor, Take};
use gitcroc_ui::settings_dialog::{SettingsDialog, SettingsTab};
use gitcroc_ui::strings::{conflict as text, integrate as banner_text, tabs as tab_text};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::ai_resolve::{
    Open, el, ended, gone, mock_provider, named, open, open_with, proposed, resolving, row_click,
    set_options, shown, start, tab_menu_ai, two_hunk_conflict, workspace_ai,
};
use crate::changes::switch_to;
use crate::conflict_flow::{job_end, on_disk, take, unmerged, value, wait_for_editor};
use crate::history::{click, label_of, press, wait_for_element, wait_until};
use crate::stage::{disabled, focused};
use crate::support::{AiScript, Fixture};

const RESOLVE_ALL: &str = "conflict-banner-resolve-ai";
const CANCEL_ALL: &str = "conflict-banner-cancel-ai";

/// The files of the default exclude list's `.env*` say why they are not
/// sent.
fn excluded(pattern: &str) -> String {
    text::not_sent(&format!("matches the exclude list ({pattern})"))
}

/// The files of the banner's progress and where each is, in order.
fn progress(cx: &mut VisualTestContext) -> Vec<(String, String)> {
    (0..)
        .map_while(|at| {
            let name = label_of(cx, named("conflict-ai-file-name", at))?;
            let state = label_of(cx, named("conflict-ai-file-state", at))?;
            Some((name, state))
        })
        .collect()
}

/// Where `path` is in the progress, once it is listed.
fn state_of(cx: &mut VisualTestContext, path: &str) -> Option<String> {
    progress(cx)
        .into_iter()
        .find(|(name, _)| name == path)
        .map(|(_, state)| state)
}

/// The place of `path` in the progress.
fn index_of(cx: &mut VisualTestContext, path: &str) -> usize {
    progress(cx)
        .iter()
        .position(|(name, _)| name == path)
        .unwrap_or_else(|| panic!("{path} is not in the progress"))
}

/// Clicks Resolve all with AI once it can be pressed.
fn resolve_all(cx: &mut VisualTestContext) {
    wait_until(cx, |cx| {
        shown(cx, el(RESOLVE_ALL)) && !disabled(cx, el(RESOLVE_ALL))
    });
    click(cx, el(RESOLVE_ALL));
}

/// Opens `path` from the progress and waits for its editor.
fn open_from_progress(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    path: &str,
) -> Entity<ConflictEditor> {
    let at = index_of(cx, path);
    click(cx, named("conflict-ai-file-name", at));
    wait_for_editor(cx, window, path)
}

/// Opens each of `paths` in turn, so each has a kept editor, and returns
/// the editors with their buffers.
fn open_each(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    paths: &[&str],
) -> Vec<(Entity<ConflictEditor>, String)> {
    paths
        .iter()
        .map(|path| {
            row_click(cx, window, path);
            let editor = wait_for_editor(cx, window, path);
            let buffer = value(cx, &editor);
            (editor, buffer)
        })
        .collect()
}

fn done_one() -> String {
    text::done_with(1, 0, 0, 0)
}

/// A second workspace, "Java", with no repos; `mine` stays active.
fn second_tab(f: &Fixture, mine: &WorkspaceId) -> WorkspaceId {
    let app = f.app().clone();
    let java = f.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    f.runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();
    java
}

/// A held run of two files: both requests have reached the mock.
fn held_run(f: &Fixture, cx: &mut VisualTestContext) -> gitcroc_ai::MockGate {
    resolve_all(cx);
    wait_until(cx, |_| !f.ai_gates().is_empty());
    let gate = f.ai_gates().remove(0);
    f.runtime.block_on(gate.arrived());
    f.runtime.block_on(gate.arrived());
    wait_for_element(cx, el(CANCEL_ALL));
    gate
}

fn two_replies() -> AiScript {
    AiScript {
        replies: vec![resolving(&[(1, "both\n")]), resolving(&[(1, "both\n")])],
        ..AiScript::default()
    }
}

/// AI-3: with no provider configured, Resolve all with AI opens Settings at
/// its AI tab.
#[gpui::test]
fn ai_3_banner_resolve_all_opens_settings_without_a_provider(root: &mut TestAppContext) {
    let Open {
        f: _f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, start(AiScript::default()), &["a.txt", "b.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    resolve_all(&mut cx);
    wait_for_element(&mut cx, "settings-dialog".into());
    let at = cx.update(|_, cx| {
        window
            .read(cx)
            .dialog_of::<SettingsDialog>()
            .map(|dialog| dialog.read(cx).tab())
    });
    assert_eq!(at, Some(SettingsTab::Ai));
}

/// AI-13: the progress lists every file, the excluded one with its reason
/// and never sent; a file's name opens it with its proposal.
#[gpui::test]
fn ai_13_progress_lists_each_file_and_opens_one_with_its_proposal(root: &mut TestAppContext) {
    let f = start(two_replies());
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt", "b.txt", ".env"]);
    wait_for_editor(&mut cx, &window, ".env");
    assert_eq!(ended(&f, &mut cx, resolve_all), Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| {
        state_of(cx, "a.txt") == Some(done_one()) && state_of(cx, "b.txt") == Some(done_one())
    });
    assert_eq!(state_of(&mut cx, ".env"), Some(excluded(".env*")));
    let requests = f.ai_requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(
        requests
            .iter()
            .all(|request| !request.user.contains(".env")),
        "{requests:?}"
    );
    let label = label_of(&mut cx, el("conflict-ai-run-label")).unwrap();
    assert!(label.starts_with(banner_text::RESOLVE_ALL), "{label}");

    // The job list shows the same files, where each is.
    click(&mut cx, "status-jobs".into());
    wait_for_element(&mut cx, el("job-ai-files"));
    let listed: Vec<(String, String)> = (0..)
        .map_while(|at| {
            let name = label_of(&mut cx, named("job-ai-file-name", at))?;
            let state = label_of(&mut cx, named("job-ai-file-state", at))?;
            Some((name, state))
        })
        .collect();
    assert_eq!(listed, progress(&mut cx));
    press(&mut cx, "escape");
    gone(&mut cx, "job-list");

    let b = open_from_progress(&mut cx, &window, "b.txt");
    wait_until(&mut cx, |cx| proposed(cx, &b) == 1);
    wait_for_element(&mut cx, el("conflict-proposal-0"));
}

/// AI-13: one reply released, then Cancel: the finished file keeps its
/// proposal, the other is cancelled with none and its buffer unchanged.
#[gpui::test]
fn ai_13_cancel_shows_cancelled_files_and_keeps_finished_proposals(root: &mut TestAppContext) {
    let f = start(AiScript {
        held: true,
        ..two_replies()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        r,
    } = open(root, f, &["a.txt", "b.txt"]);
    let editors = open_each(&mut cx, &window, &["a.txt", "b.txt"]);
    let gate = held_run(&f, &mut cx);
    gate.release();
    wait_until(&mut cx, |cx| {
        progress(cx).iter().any(|(_, state)| *state == done_one())
    });
    let mut events = f.app().subscribe();
    click(&mut cx, el(CANCEL_ALL));
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| {
        progress(cx)
            .iter()
            .any(|(_, state)| state == text::CANCELLED)
    });
    let states = progress(&mut cx);
    let done = states
        .iter()
        .find(|(_, state)| *state == done_one())
        .map(|(path, _)| path.clone())
        .unwrap();
    let cancelled = states
        .iter()
        .find(|(_, state)| state == text::CANCELLED)
        .map(|(path, _)| path.clone())
        .unwrap();
    wait_for_element(&mut cx, el(RESOLVE_ALL));

    let at = |path: &str| usize::from(path == "b.txt");
    let (kept, _) = &editors[at(&done)];
    wait_until(&mut cx, |cx| proposed(cx, kept) == 1);
    let (dropped, buffer) = &editors[at(&cancelled)];
    assert_eq!(proposed(&mut cx, dropped), 0);
    assert_eq!(&value(&mut cx, dropped), buffer);
    open_from_progress(&mut cx, &window, &cancelled);
    assert!(!shown(&mut cx, el("conflict-proposal-0")));
    assert!(on_disk(&r, &cancelled).contains("<<<<<<<"));
}

/// AI-13, OPS-9: a file with an editor sends only the hunks its buffer has
/// untouched.
#[gpui::test]
fn ai_13_buffers_send_only_their_untouched_hunks(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(2, "both TWO\n")])],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open_with(root, f, two_hunk_conflict);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Ours);
    assert_eq!(ended(&f, &mut cx, resolve_all), Some(Outcome::Succeeded));
    let requests = f.ai_requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let sent = &requests[0].user;
    assert!(sent.contains("topic TWO"), "{sent}");
    assert!(!sent.contains("topic ONE"), "{sent}");
}

/// UI-30: the run goes on while its tab is in the background, and its
/// proposals are there when the tab is shown again.
#[gpui::test]
fn ai_13_the_run_goes_on_in_a_background_tab(root: &mut TestAppContext) {
    let f = start(AiScript {
        held: true,
        ..two_replies()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt", "b.txt"]);
    let a = wait_for_editor(&mut cx, &window, "a.txt");
    let mine = f.active();
    let java = second_tab(&f, &mine);
    let gate = held_run(&f, &mut cx);

    switch_to(&mut cx, &f, &window, &java);
    let mut events = f.app().subscribe();
    gate.release();
    gate.release();
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));

    switch_to(&mut cx, &f, &window, &mine);
    wait_until(&mut cx, |cx| {
        state_of(cx, "a.txt") == Some(done_one()) && state_of(cx, "b.txt") == Some(done_one())
    });
    wait_until(&mut cx, |cx| proposed(cx, &a) == 1);
    let b = open_from_progress(&mut cx, &window, "b.txt");
    wait_until(&mut cx, |cx| proposed(cx, &b) == 1);
}

/// AI-6: AI for this workspace off disables Resolve all with the reason.
#[gpui::test]
fn ai_6_resolve_all_disabled_with_the_reason_when_the_workspace_switch_is_off(
    root: &mut TestAppContext,
) {
    let f = start(AiScript::default());
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    let id = f.active();
    wait_for_element(&mut cx, el(RESOLVE_ALL));
    assert!(!disabled(&mut cx, el(RESOLVE_ALL)));

    assert_eq!(tab_menu_ai(&mut cx, &id), Some(true));
    crate::history::click_menu(&mut cx, tab_text::AI);
    wait_until(&mut cx, |_| !workspace_ai(&f, &id));
    wait_until(&mut cx, |cx| {
        label_of(cx, el("conflict-banner-ai-reason")).as_deref() == Some(text::AI_OFF_HERE)
    });
    assert!(disabled(&mut cx, el(RESOLVE_ALL)));
    click(&mut cx, el(RESOLVE_ALL));
    assert!(f.ai_requests().is_empty());

    assert_eq!(tab_menu_ai(&mut cx, &id), Some(false));
    crate::history::click_menu(&mut cx, tab_text::AI);
    wait_until(&mut cx, |_| workspace_ai(&f, &id));
    gone(&mut cx, "conflict-banner-ai-reason");
    assert!(!disabled(&mut cx, el(RESOLVE_ALL)));
}

/// D9, AI-12: after the run nothing on disk or in the index changed, and
/// every buffer is as it was; each file still needs Accept.
#[gpui::test]
fn d9_resolve_all_accepts_nothing(root: &mut TestAppContext) {
    let f = start(two_replies());
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        r,
    } = open(root, f, &["a.txt", "b.txt"]);
    let editors = open_each(&mut cx, &window, &["a.txt", "b.txt"]);
    let (disk_a, disk_b, index) = (on_disk(&r, "a.txt"), on_disk(&r, "b.txt"), unmerged(&r));
    assert_eq!(ended(&f, &mut cx, resolve_all), Some(Outcome::Succeeded));
    for (editor, _) in &editors {
        wait_until(&mut cx, |cx| proposed(cx, editor) == 1);
    }
    for (editor, buffer) in &editors {
        assert_eq!(&value(&mut cx, editor), buffer);
    }
    assert_eq!(on_disk(&r, "a.txt"), disk_a);
    assert_eq!(on_disk(&r, "b.txt"), disk_b);
    assert_eq!(unmerged(&r), index);
    assert!(disabled(&mut cx, el("conflict-banner-continue")));
}

/// AI-16: AI features off hides Resolve all; off during a run ends it, and
/// no proposal is kept.
#[gpui::test]
fn ai_16_off_hides_resolve_all(root: &mut TestAppContext) {
    let f = start(AiScript {
        held: true,
        ..two_replies()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt", "b.txt"]);
    let a = wait_for_editor(&mut cx, &window, "a.txt");
    held_run(&f, &mut cx);
    wait_for_element(&mut cx, el("conflict-ai-files"));

    let mut events = f.app().subscribe();
    set_options(&f, |options| options.enabled = false);
    job_end(&mut cx, &mut events);
    gone(&mut cx, "conflict-ai-files");
    assert!(!shown(&mut cx, el(RESOLVE_ALL)));
    assert!(!shown(&mut cx, el(CANCEL_ALL)));

    set_options(&f, |options| options.enabled = true);
    wait_for_element(&mut cx, el(RESOLVE_ALL));
    assert!(!shown(&mut cx, el("conflict-ai-files")));
    assert_eq!(proposed(&mut cx, &a), 0);
    row_click(&mut cx, &window, "b.txt");
    let b = wait_for_editor(&mut cx, &window, "b.txt");
    assert_eq!(proposed(&mut cx, &b), 0);
}

/// SEC-3: file paths and reasons render as text.
#[gpui::test]
fn sec_3_file_paths_and_reasons_render_as_text(root: &mut TestAppContext) {
    let path = "x &amp; <b>y.txt";
    let path = if cfg!(windows) { "x &amp; y.txt" } else { path };
    let f = start(AiScript::default());
    mock_provider(&f, "Mock", true);
    let pattern = "*&amp;*";
    set_options(&f, |options| options.exclude = vec![pattern.to_owned()]);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &[path]);
    wait_for_editor(&mut cx, &window, path);
    ended(&f, &mut cx, resolve_all);
    wait_until(&mut cx, |cx| state_of(cx, path).is_some());
    assert_eq!(state_of(&mut cx, path), Some(excluded(pattern)));
    assert_eq!(
        label_of(&mut cx, named("conflict-ai-file-name", 0)).as_deref(),
        Some(path)
    );
    assert!(f.ai_requests().is_empty());
}

/// UI-22: Tab reaches Resolve all with AI and Space presses it; Tab reaches
/// a file of the progress and Space opens it.
#[gpui::test]
fn ui_22_resolve_all_and_the_progress_list_are_keyboard_reachable(root: &mut TestAppContext) {
    let f = start(two_replies());
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt", "b.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    wait_for_element(&mut cx, el(RESOLVE_ALL));
    let reach = |cx: &mut VisualTestContext, id: gpui_kit::ElementId| {
        click(cx, "history-uncommitted".into());
        for _ in 0..40 {
            if focused(cx, id.clone()) {
                return;
            }
            press(cx, "tab");
        }
        panic!("Tab never reached {id:?}");
    };
    reach(&mut cx, el(RESOLVE_ALL));
    let outcome = ended(&f, &mut cx, |cx| press(cx, "space"));
    assert_eq!(outcome, Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| state_of(cx, "b.txt") == Some(done_one()));
    let at = index_of(&mut cx, "b.txt");
    reach(&mut cx, named("conflict-ai-file-name", at));
    press(&mut cx, "space");
    let b = wait_for_editor(&mut cx, &window, "b.txt");
    wait_until(&mut cx, |cx| proposed(cx, &b) == 1);
}
