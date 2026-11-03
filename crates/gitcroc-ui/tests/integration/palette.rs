//! M6-24a: the command palette (UI-22), against the real core on repo-gen
//! fixtures (QA-2).

use crate::changes::show_repo;
use crate::history::{
    add, click, label_of, labels_in, press, repo, repos, wait_for_element, wait_until,
};
use crate::support::{Fixture, main_window};
use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{PaletteItem, PaletteResult};
use gitcroc_ui::palette::Palette;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext, point, px};

/// The palette's key on the platform the tests run on.
pub(crate) fn palette_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-k"
    } else {
        "ctrl-k"
    }
}

pub(crate) fn palette_of(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
) -> Option<Entity<Palette>> {
    cx.update(|_, cx| window.read(cx).dialog_of::<Palette>())
}

/// The answer the palette shows.
pub(crate) fn result(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
) -> Option<PaletteResult> {
    let palette = palette_of(cx, window)?;
    cx.update(|_, cx| palette.read(cx).result().cloned())
}

/// The text of `item`'s row.
pub(crate) fn text_of(item: &PaletteItem) -> &str {
    match item {
        PaletteItem::Operation { label, .. } => label,
        PaletteItem::Repo { name, .. }
        | PaletteItem::Group { name, .. }
        | PaletteItem::Branch { name, .. } => name,
    }
}

/// Opens the palette by its key and waits for its first answer.
pub(crate) fn open_palette(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>) {
    press(cx, palette_key());
    wait_for_element(cx, "palette".into());
    for _ in 0..200 {
        crate::support::settle(cx, |_, _| true);
        if result(cx, window).is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!(
        "no answer: error {:?}, sent {:?}",
        label_of(cx, "palette-error".into()),
        palette_of(cx, window).map(|p| cx.update(|_, cx| p.read(cx).sent()))
    );
}

/// Types `text` into the palette and waits until its answer's first row
/// is `first`.
pub(crate) fn search(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    text: &str,
    first: &str,
) {
    cx.update(|window, cx| window.input(text, cx));
    wait_until(cx, |cx| {
        result(cx, window)
            .and_then(|result| result.items.first().map(|item| text_of(item) == first))
            .unwrap_or(false)
    });
}

fn row(at: usize) -> ElementId {
    ElementId::NamedInteger("palette-row".into(), at as u64)
}

fn part(at: usize, part: &str) -> ElementId {
    ElementId::Name(format!("palette-row-{at}-{part}").into())
}

fn focused(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.focused()) == Some(true)
    })
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// The window on repos `names`, the first one shown.
pub(crate) struct Opened {
    pub f: Fixture,
    pub made: repo_gen::Fixture,
    pub cx: VisualTestContext,
    pub window: Entity<GitcrocWindow>,
}

pub(crate) fn opened(root: &mut TestAppContext, names: &[&str]) -> Opened {
    let f = Fixture::start(EVENT_CAPACITY);
    let made = repos(names);
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    crate::stage::enlarge(&mut cx);
    show_repo(&mut cx, &window, &added, names[0]);
    Opened {
        f,
        made,
        cx,
        window,
    }
}

/// Empties the palette's text box.
pub(crate) fn clear(cx: &mut VisualTestContext) {
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
    );
    press(cx, "backspace");
}

#[gpui::test]
fn ui_22_palette_opens_by_key_and_by_button_and_closes_giving_the_focus_back(
    root: &mut TestAppContext,
) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api"]);
    assert!(focused(&mut cx, "tree-list"));
    open_palette(&mut cx, &window);
    assert!(focused(&mut cx, "palette-input"));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "palette"));
    assert!(focused(&mut cx, "tree-list"));
    // The key again closes it.
    open_palette(&mut cx, &window);
    press(&mut cx, palette_key());
    assert!(!shown(&mut cx, "palette"));
    assert!(focused(&mut cx, "tree-list"));
    // The toolbar's button opens it.
    click(&mut cx, "toolbar-palette".into());
    wait_for_element(&mut cx, "palette".into());
    assert!(focused(&mut cx, "palette-input"));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "palette"));
}

#[gpui::test]
fn ui_22_a_click_outside_the_palette_closes_it_and_one_inside_does_not(root: &mut TestAppContext) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api"]);
    open_palette(&mut cx, &window);
    click(&mut cx, "palette-list".into());
    assert!(shown(&mut cx, "palette"));
    click(&mut cx, "palette-input".into());
    assert!(shown(&mut cx, "palette"));
    cx.update(|window, cx| window.click_at("dialog-layer", point(px(4.), px(4.)), cx));
    cx.run_until_parked();
    assert!(!shown(&mut cx, "palette"));
    // The click did nothing else: the tree keeps its selection.
    assert!(focused(&mut cx, "tree-list"));
}

#[gpui::test]
fn ui_22_the_keys_move_the_highlight_and_enter_runs_it(root: &mut TestAppContext) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api"]);
    open_palette(&mut cx, &window);
    let palette = palette_of(&mut cx, &window).unwrap();
    let highlight = |cx: &mut VisualTestContext| cx.update(|_, cx| palette.read(cx).highlight());
    let count = result(&mut cx, &window).unwrap().items.len();
    assert!(count > 20, "{count} rows");
    press(&mut cx, "down");
    press(&mut cx, "down");
    assert_eq!(highlight(&mut cx), 2);
    press(&mut cx, "up");
    assert_eq!(highlight(&mut cx), 1);
    press(&mut cx, "pagedown");
    assert_eq!(highlight(&mut cx), 12);
    press(&mut cx, "pageup");
    assert_eq!(highlight(&mut cx), 1);
    press(&mut cx, "end");
    assert_eq!(highlight(&mut cx), count - 1);
    // The highlighted row is kept in view.
    assert_eq!(
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.find(row(count - 1)).selected()
        }),
        Some(true)
    );
    press(&mut cx, "home");
    assert_eq!(highlight(&mut cx), 0);
    // Enter runs the highlighted row: Settings.
    search(&mut cx, &window, "Settings", "Settings");
    press(&mut cx, "enter");
    assert!(!shown(&mut cx, "palette"));
    wait_for_element(&mut cx, "settings-dialog".into());
}

#[gpui::test]
fn ui_22_a_line_says_when_more_matched_than_are_shown(root: &mut TestAppContext) {
    let names: Vec<String> = (0..120).map(|at| format!("svc{at:03}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &names);
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "svc", "svc000");
    let answer = result(&mut cx, &window).unwrap();
    assert_eq!(answer.items.len(), 100);
    assert!(answer.total > 100, "{}", answer.total);
    assert_eq!(
        label_of(&mut cx, "palette-more".into()),
        Some(format!("100 of {}, keep typing", answer.total))
    );
    clear(&mut cx);
    cx.update(|window, cx| window.input("zzz-nothing", cx));
    wait_until(&mut cx, |cx| {
        label_of(cx, "palette-more".into()).as_deref() == Some("Nothing matches")
    });
}

#[gpui::test]
fn ui_22_an_operation_with_a_shortcut_shows_its_keys(root: &mut TestAppContext) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api"]);
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "Next tab", "Next tab");
    let keys = if cfg!(target_os = "macos") {
        "Cmd+Page Down"
    } else {
        "Ctrl+Page Down"
    };
    assert_eq!(
        label_of(&mut cx, part(0, "shortcut")),
        Some(keys.to_owned())
    );
}

#[gpui::test]
fn perf_1_the_palette_list_lays_out_only_the_visible_rows(root: &mut TestAppContext) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api"]);
    open_palette(&mut cx, &window);
    let count = result(&mut cx, &window).unwrap().items.len();
    let laid_out = labels_in(&mut cx, "palette-list".into(), Role::ListBoxOption).len();
    assert!(count > 40, "{count} rows");
    assert!(
        laid_out > 0 && laid_out < 20,
        "{laid_out} of {count} laid out"
    );
}

#[gpui::test]
fn sec_3_palette_names_render_as_text(root: &mut TestAppContext) {
    let f = Fixture::start(EVENT_CAPACITY);
    let name = "a&amp;b";
    let branch = "<b>bold&amp;";
    let made = repos(&[name]);
    // A loose ref is a file, and Windows forbids `<` in file names, so the
    // branch goes in `packed-refs`, where git keeps a fetched one.
    let hash = repo(&made, name).head().unwrap();
    std::fs::write(
        made.path().join(name).join(".git").join("packed-refs"),
        format!("{hash} refs/heads/{branch}\n"),
    )
    .unwrap();
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    crate::stage::enlarge(&mut cx);
    show_repo(&mut cx, &window, &added, name);
    open_palette(&mut cx, &window);
    search(&mut cx, &window, name, name);
    assert_eq!(label_of(&mut cx, part(0, "text")), Some(name.to_owned()));
    clear(&mut cx);
    search(&mut cx, &window, branch, branch);
    assert_eq!(label_of(&mut cx, part(0, "text")), Some(branch.to_owned()));
    assert_eq!(
        label_of(&mut cx, part(0, "detail")),
        Some(format!("in {name}"))
    );
}

#[gpui::test]
fn ui_22_the_palette_opens_from_inside_the_conflict_editor(root: &mut TestAppContext) {
    use crate::ai_resolve::{Open, open, start};
    let Open {
        f: _f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, start(crate::support::AiScript::default()), &["a.txt"]);
    let editor = crate::conflict_flow::wait_for_editor(&mut cx, &window, "a.txt");
    cx.update(|window, cx| {
        let focus = gpui_kit::Focusable::focus_handle(editor.read(cx).editor.read(cx), cx);
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    open_palette(&mut cx, &window);
    assert!(focused(&mut cx, "palette-input"));
    press(&mut cx, "escape");
    assert!(!shown(&mut cx, "palette"));
    let back = cx.update(|window, cx| {
        let focus = gpui_kit::Focusable::focus_handle(editor.read(cx).editor.read(cx), cx);
        focus.is_focused(window)
    });
    assert!(back, "the focus went back to the editor");
}

#[gpui::test]
fn ui_22_the_palette_keeps_the_last_answer_and_drops_a_superseded_one(root: &mut TestAppContext) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api", "web"]);
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "api", "api");
    let shown_before = result(&mut cx, &window).unwrap();
    // Typing asks again; until the answer arrives the list keeps the last
    // one, so typing never waits.
    cx.update(|window, cx| {
        window.input("x", cx);
        window.render_frame(cx);
    });
    assert_eq!(result(&mut cx, &window), Some(shown_before.clone()));
    // Two more changes at once: the core supersedes the first, and only
    // the newest text's answer is shown.
    clear(&mut cx);
    cx.update(|window, cx| {
        window.input("we", cx);
        window.input("b", cx);
    });
    wait_until(&mut cx, |cx| {
        result(cx, &window)
            .and_then(|result| result.items.first().map(|item| text_of(item) == "web"))
            .unwrap_or(false)
    });
    // An answer older than the one shown is dropped.
    let palette = palette_of(&mut cx, &window).unwrap();
    let newest = cx.update(|_, cx| palette.read(cx).result().cloned());
    cx.update(|_, cx| {
        palette.update(cx, |palette, cx| {
            // The first request's, long since answered.
            palette.answer(1, Ok(Some(shown_before)), cx);
        })
    });
    assert_eq!(
        cx.update(|_, cx| palette.read(cx).result().cloned()),
        newest
    );
}

#[gpui::test]
fn ui_22_an_operation_that_cannot_apply_shows_why_and_enter_does_nothing(
    root: &mut TestAppContext,
) {
    let Opened {
        f: _f,
        made: _made,
        mut cx,
        window,
    } = opened(root, &["api"]);
    // Disabled by its control's rule: nothing is staged.
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "Commit…", "Commit…");
    wait_until(&mut cx, |cx| label_of(cx, part(0, "reason")).is_some());
    let reason = label_of(&mut cx, part(0, "reason"));
    assert!(reason.is_some(), "{reason:?}");
    press(&mut cx, "enter");
    assert!(shown(&mut cx, "palette"));
    // Missing its subject: a repo is selected, not a branch.
    clear(&mut cx);
    search(&mut cx, &window, "Rename branch…", "Rename branch…");
    assert_eq!(
        label_of(&mut cx, part(0, "reason")),
        Some("Select a branch in the tree".to_owned())
    );
    press(&mut cx, "enter");
    assert!(shown(&mut cx, "palette"));
    assert!(!shown(&mut cx, "branch-dialog"));
}

#[gpui::test]
fn ui_22_the_palette_selects_a_repo_a_group_and_a_remote_branch(root: &mut TestAppContext) {
    use crate::group_ops::Setup;
    let mut setup = Setup::new(&[
        ("api", crate::group_ops::Remote::Ok),
        ("web", crate::group_ops::Remote::Ok),
    ]);
    // As a fetch leaves it.
    repo(&setup.made, "web")
        .git(["update-ref", "refs/remotes/origin/feature/far", "HEAD"])
        .unwrap();
    setup.add();
    let group = setup.group("Backend", &["api"]);
    let (mut cx, window) = crate::group_ops::open(root, &setup, &group);
    let web = setup.id("web");
    let selected = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let tree = window.read(cx).tree(cx).unwrap();
            tree.read(cx).selected_item().cloned()
        })
    };
    // A repo.
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "web", "web");
    press(&mut cx, "enter");
    assert!(!shown(&mut cx, "palette"));
    wait_until(
        &mut cx,
        |cx| matches!(selected(cx), Some(gitcroc_core::TreeItem::Repo { repo_id, .. }) if repo_id == web),
    );
    assert!(focused(&mut cx, "tree-list"));
    wait_until(&mut cx, |cx| {
        crate::history::with_history(cx, &window, |history| {
            history
                .source()
                .is_some_and(|source| source.repo.as_ref() == Some(&web))
        })
    });
    // A group.
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "Backend", "Backend");
    press(&mut cx, "enter");
    wait_until(
        &mut cx,
        |cx| matches!(selected(cx), Some(gitcroc_core::TreeItem::Group { group_id, .. }) if group_id == group),
    );
    // A remote branch, whose repo and folders expand to show it.
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "origin/feature/far", "origin/feature/far");
    press(&mut cx, "enter");
    wait_until(&mut cx, |cx| {
        matches!(
            selected(cx),
            Some(gitcroc_core::TreeItem::RemoteBranch { repo_id, name, .. })
                if repo_id == web && name == "origin/feature/far"
        )
    });
    assert!(focused(&mut cx, "tree-list"));
}

#[gpui::test]
fn ai_3_resolve_with_ai_from_the_palette_opens_settings_without_a_provider(
    root: &mut TestAppContext,
) {
    use crate::ai_resolve::{Open, open, start};
    use gitcroc_ui::settings_dialog::{SettingsDialog, SettingsTab};
    let Open {
        f: _f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, start(crate::support::AiScript::default()), &["a.txt"]);
    crate::conflict_flow::wait_for_editor(&mut cx, &window, "a.txt");
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "Resolve with AI", "Resolve with AI");
    wait_until(&mut cx, |cx| label_of(cx, part(0, "reason")).is_none());
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "settings-dialog".into());
    let at = cx.update(|_, cx| {
        window
            .read(cx)
            .dialog_of::<SettingsDialog>()
            .map(|dialog| dialog.read(cx).tab())
    });
    assert_eq!(at, Some(SettingsTab::Ai));
}

#[gpui::test]
fn ai_16_the_palette_has_no_ai_entries_while_ai_is_off(root: &mut TestAppContext) {
    use crate::ai_resolve::{Open, open, set_options, start};
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, start(crate::support::AiScript::default()), &["a.txt"]);
    crate::conflict_flow::wait_for_editor(&mut cx, &window, "a.txt");
    let ai_rows = |cx: &mut VisualTestContext| {
        let ai: Vec<&str> = gitcroc_core::palette_catalog()
            .iter()
            .filter(|entry| entry.ai)
            .map(|entry| entry.label)
            .collect();
        result(cx, &window)
            .unwrap()
            .items
            .iter()
            .filter(|item| ai.contains(&text_of(item)))
            .count()
    };
    open_palette(&mut cx, &window);
    assert!(ai_rows(&mut cx) > 0);
    press(&mut cx, "escape");
    set_options(&f, |options| options.enabled = false);
    open_palette(&mut cx, &window);
    assert_eq!(ai_rows(&mut cx), 0);
}

#[gpui::test]
fn ui_22_fetch_on_a_group_goes_through_its_preflight_and_a_hard_reset_asks_first(
    root: &mut TestAppContext,
) {
    use crate::group_ops::{Remote, Setup, open, wait_preflight_ready};
    let mut setup = Setup::new(&[("api", Remote::Ok), ("docs", Remote::None)]);
    let api = repo(&setup.made, "api");
    let first = api.head().unwrap();
    api.commit_file("two.txt", "two\n", "Second").unwrap();
    let head = api.head().unwrap();
    setup.add();
    let group = setup.group("Backend", &["api", "docs"]);
    let (mut cx, window) = open(root, &setup, &group);
    // As the toolbar's Fetch: a repo that will skip asks first.
    let jobs = setup.jobs();
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "Fetch", "Fetch");
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "preflight".into());
    wait_preflight_ready(&mut cx);
    assert_eq!(
        label_of(&mut cx, "preflight-counts".into()).as_deref(),
        Some("1 will run, 1 will skip")
    );
    assert_eq!(setup.jobs(), jobs, "nothing ran before Run");
    press(&mut cx, "escape");
    // A hard reset asks first, as the commit menu's Reset does.
    open_palette(&mut cx, &window);
    search(&mut cx, &window, "api", "api");
    press(&mut cx, "enter");
    crate::history::wait_for_rows(&mut cx, &window, 2);
    click(&mut cx, "history-row-1".into());
    open_palette(&mut cx, &window);
    let hard = "Reset current branch: hard…";
    search(&mut cx, &window, hard, hard);
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "operation-dialog".into());
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "op-confirm-actions".into()).is_some()
    });
    assert_eq!(
        api.head().unwrap(),
        head,
        "nothing was reset before the answer"
    );
    click(&mut cx, "dialog-submit".into());
    wait_until(&mut cx, |_| api.head().unwrap() == first);
}
