//! M6-19a: actual editor transactions, key dispatch and retained entities.
use super::support::open;
use gitcroc_core::{ConflictChunk, ConflictLabels, RepoId, WorkspaceId};
use gitcroc_ui::conflict::{Buffer, ConflictEditor, EditorKey, Editors, Take};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, prelude::*, px,
};
use gpui_kit::{Window, div};

struct Host {
    view: Entity<ConflictEditor>,
    visible: bool,
    presses: usize,
    focus: gpui_kit::FocusHandle,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                gpui_component::button::Button::new("other-control")
                    .label("Other")
                    .track_focus(&self.focus)
                    .on_click(cx.listener(|this, _, _, _| this.presses += 1)),
            )
            .when(self.visible, |el| {
                el.child(div().flex_1().child(self.view.clone()))
            })
    }
}
fn hosted(
    cx: &mut TestAppContext,
    buffer: Buffer,
) -> (VisualTestContext, Entity<Host>, Entity<ConflictEditor>) {
    let (mut cx, host) = open(cx, |window, cx| {
        cx.new(|cx| Host {
            view: cx.new(|cx| ConflictEditor::new(buffer, window, cx)),
            visible: true,
            presses: 0,
            focus: cx.focus_handle(),
        })
    });
    let view = cx.update(|_, cx| host.read(cx).view.clone());
    (cx, host, view)
}
fn visibility(cx: &mut VisualTestContext, host: &Entity<Host>, visible: bool) {
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.view.update(cx, |view, cx| {
                if visible {
                    view.show(window, cx)
                } else {
                    view.hide(window, cx)
                }
            });
            host.visible = visible;
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

fn labels() -> ConflictLabels {
    ConflictLabels {
        ours: "main\r\nbranch".into(),
        theirs: "<feature>".into(),
    }
}
fn chunk(ours: &str, theirs: &str) -> ConflictChunk {
    ConflictChunk::Hunk {
        ours: ours.into(),
        base: "base\r\n".into(),
        theirs: theirs.into(),
    }
}
fn buffer(prefix: &str, ours: &str, suffix: &str) -> Buffer {
    Buffer::build(
        &[
            ConflictChunk::Resolved {
                text: prefix.into(),
            },
            chunk(ours, "他"),
            ConflictChunk::Resolved {
                text: suffix.into(),
            },
        ],
        &labels(),
    )
}
fn editor(cx: &mut TestAppContext, buffer: Buffer) -> (VisualTestContext, Entity<ConflictEditor>) {
    open(cx, |window, cx| {
        cx.new(|cx| ConflictEditor::new(buffer, window, cx))
    })
}
fn value(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>) -> String {
    cx.update(|_, cx| view.read(cx).value(cx))
}
fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|window, cx| window.press(key, cx));
    cx.run_until_parked();
}
fn undo(cx: &mut VisualTestContext) {
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-z"
        } else {
            "ctrl-z"
        },
    );
}
fn redo(cx: &mut VisualTestContext) {
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-shift-z"
        } else {
            "ctrl-y"
        },
    );
}
fn edit(
    cx: &mut VisualTestContext,
    view: &Entity<ConflictEditor>,
    range: std::ops::Range<usize>,
    text: &str,
) {
    cx.update(|window, cx| {
        let state = view.read(cx).editor.clone();
        state.update(cx, |state, cx| {
            state.set_selected_range(range, cx);
            state.replace(text.to_owned(), window, cx);
            state.focus(window, cx);
        });
    });
    cx.run_until_parked();
}
fn take(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>, id: usize, side: Take) -> bool {
    let result =
        cx.update(|window, cx| view.update(cx, |view, cx| view.take(id, side, window, cx)));
    cx.run_until_parked();
    result
}

#[test]
fn ops_8_buffer_uses_core_chunks_and_real_labels() {
    let b = buffer("<<<<<<< misleading\r\n🦀\r", "ours", "tail");
    assert_eq!(b.hunks.len(), 1);
    assert!(b.text.starts_with(
        "<<<<<<< misleading\n🦀\n<<<<<<< main branch\nours\n=======\n他\n>>>>>>> <feature>\n"
    ));
    assert!(b.text.ends_with("tail"));
    let h = &b.hunks[0];
    assert_eq!(&b.text[h.range.clone()], h.original);
    assert_eq!(h.base, "base\n");
    assert_eq!(h.taken(Take::Both), "ours\n他");
    assert_eq!(buffer("", "", "").hunks[0].taken(Take::Both), "他");
}

#[gpui::test]
fn ops_3_take_ours_theirs_both_is_one_undo_step(cx: &mut TestAppContext) {
    for (prefix, ours, suffix) in [
        ("", "", ""),
        ("🦀", "", "tail"),
        ("head\n", "ours", ""),
        ("", "ours", "tail"),
    ] {
        for side in [Take::Ours, Take::Theirs, Take::Both] {
            let b = buffer(prefix, ours, suffix);
            let original = b.text.clone();
            let expected = format!("{prefix}{}{suffix}", b.hunks[0].taken(side));
            let (mut cx, view) = editor(cx, b);
            assert!(take(&mut cx, &view, 0, side));
            assert_eq!(value(&mut cx, &view), expected);
            undo(&mut cx);
            assert_eq!(value(&mut cx, &view), original);
            assert!(cx.update(|_, cx| view.read(cx).ranges(cx)[0].2));
            redo(&mut cx);
            assert_eq!(value(&mut cx, &view), expected);
            undo(&mut cx);
            assert!(take(&mut cx, &view, 0, side));
        }
    }
}

#[gpui::test]
fn ops_3_anchors_follow_edits_and_boundaries(cx: &mut TestAppContext) {
    let b = Buffer::build(
        &[
            ConflictChunk::Resolved {
                text: "🦀\n".into(),
            },
            chunk("a", "b"),
            chunk("c", "d"),
        ],
        &labels(),
    );
    let first = b.hunks[0].range.clone();
    let second = b.hunks[1].range.clone();
    let (mut cx, view) = editor(cx, b);
    for (range, text) in [
        (0..0, "前"),
        (first.start..first.start, "x"),
        (first.end..first.end, "x"),
        (first.start + 1..first.start + 2, "y"),
        (first.start..second.end, ""),
        (0..second.end, ""),
    ] {
        edit(&mut cx, &view, range.clone(), text);
        let ranges = cx.update(|_, cx| view.read(cx).ranges(cx));
        if range.is_empty() {
            assert!(ranges.iter().all(|(_, _, untouched)| *untouched));
        }
        undo(&mut cx);
        let ranges = cx.update(|_, cx| view.read(cx).ranges(cx));
        assert_eq!(
            ranges,
            vec![(0, first.clone(), true), (1, second.clone(), true)]
        );
    }
    assert!(take(&mut cx, &view, 1, Take::Ours));
    assert!(take(&mut cx, &view, 0, Take::Theirs));
    assert_eq!(value(&mut cx, &view), "🦀\nbc");
    undo(&mut cx);
    undo(&mut cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).ranges(cx)).len(), 2);
}

#[gpui::test]
fn ops_3_manual_edit_disables_actions_until_undone(cx: &mut TestAppContext) {
    let b = buffer("head\n", "ours", "tail");
    let at = b.hunks[0].range.start + b.hunks[0].head;
    let (mut cx, view) = editor(cx, b);
    edit(&mut cx, &view, at..at, "manual");
    assert!(!take(&mut cx, &view, 0, Take::Ours));
    assert_eq!(cx.update(|_, cx| view.read(cx).ranges(cx)).len(), 1);
    undo(&mut cx);
    assert!(take(&mut cx, &view, 0, Take::Ours));
}

#[gpui::test]
fn ai_12_accept_only_changes_the_untouched_buffer(cx: &mut TestAppContext) {
    let b = buffer("head\n", "ours", "tail");
    let original = b.text.clone();
    let at = b.hunks[0].range.start + 1;
    let (mut cx, view) = editor(cx, b);
    assert!(
        cx.update(|window, cx| view.update(cx, |view, cx| view.accept(0, "提案\r\n", window, cx)))
    );
    cx.run_until_parked();
    assert_eq!(value(&mut cx, &view), "head\n提案\ntail");
    undo(&mut cx);
    assert_eq!(value(&mut cx, &view), original);
    edit(&mut cx, &view, at..at, "x");
    assert!(!cx.update(|window, cx| view.update(cx, |view, cx| view.accept(0, "bad", window, cx))));
}

#[gpui::test]
fn ui_22_conflict_navigation_and_take_keys(cx: &mut TestAppContext) {
    let b = Buffer::build(
        &[
            chunk("a", "b"),
            ConflictChunk::Resolved {
                text: "line\n".repeat(200),
            },
            chunk("c", "d"),
        ],
        &labels(),
    );
    let second = b.hunks[1].range.start + b.hunks[1].head;
    let (mut cx, host, view) = hosted(cx, b);
    cx.update(|window, cx| {
        view.read(cx)
            .editor
            .clone()
            .update(cx, |state, cx| state.focus(window, cx))
    });
    press(&mut cx, "alt-down");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).editor.read(cx).cursor()),
        second
    );
    assert!(cx.update(|_, cx| view.read(cx).editor.read(cx).scroll_offset().y) < px(0.));
    press(&mut cx, "alt-down");
    press(&mut cx, "alt-3");
    assert!(value(&mut cx, &view).starts_with("a\nbline"));
    undo(&mut cx);
    press(&mut cx, "alt-down");
    press(&mut cx, "alt-2");
    assert!(value(&mut cx, &view).ends_with('d'));
    undo(&mut cx);
    cx.update(|window, cx| window.input("typed", cx));
    assert!(value(&mut cx, &view).contains("typed"));
    undo(&mut cx);
    // No plain Enter/Space binding on the wrapper: both remain editor input.
    press(&mut cx, "enter");
    press(&mut cx, "space");
    assert!(value(&mut cx, &view).contains("\n "));
    cx.update(|window, cx| host.read(cx).focus.clone().focus(window, cx));
    press(&mut cx, "enter");
    press(&mut cx, "space");
    assert_eq!(cx.update(|_, cx| host.read(cx).presses), 2);
}

fn key(workspace: &str, repo: &str, path: &str) -> EditorKey {
    EditorKey {
        workspace: WorkspaceId(workspace.into()),
        repo: RepoId(repo.into()),
        path: path.into(),
    }
}
#[gpui::test]
fn ui_29_conflict_editor_entity_keeps_cursor_scroll_and_undo(cx: &mut TestAppContext) {
    let (mut cx, host, view) = hosted(cx, buffer("head\n", "ours", &"tail\n".repeat(200)));
    let mut editors = Editors::default();
    let a = key("w", "r", "file");
    editors.keep(a.clone(), view.clone());
    edit(&mut cx, &view, 0..0, "edited");
    cx.update(|_, cx| {
        view.read(cx).editor.clone().update(cx, |state, cx| {
            state.set_selected_range(3..5, cx);
            state.set_scroll_offset(point(px(0.), px(-90.)), cx);
        })
    });
    for other in [
        key("other", "r", "file"),
        key("w", "other", "file"),
        key("w", "r", "other"),
    ] {
        assert!(editors.get(&other).is_none());
        let isolated = cx.update(|window, cx| {
            cx.new(|cx| ConflictEditor::new(buffer("", "isolated", ""), window, cx))
        });
        editors.keep(other.clone(), isolated.clone());
        assert!(take(&mut cx, &isolated, 0, Take::Ours));
        assert_eq!(value(&mut cx, &isolated), "isolated");
        assert!(value(&mut cx, &view).starts_with("editedhead"));
        visibility(&mut cx, &host, false);
        cx.update(|_, cx| {
            host.update(cx, |host, cx| {
                host.view = isolated;
                cx.notify();
            })
        });
        visibility(&mut cx, &host, true);
        visibility(&mut cx, &host, false);
        cx.update(|_, cx| {
            host.update(cx, |host, cx| {
                host.view = view.clone();
                cx.notify();
            })
        });
        visibility(&mut cx, &host, true);
    }
    cx.update(|window, cx| {
        view.read(cx)
            .editor
            .clone()
            .update(cx, |state, cx| state.focus(window, cx))
    });
    visibility(&mut cx, &host, false);
    visibility(&mut cx, &host, true);
    assert!(cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window)));
    visibility(&mut cx, &host, false);
    cx.update(|window, cx| host.read(cx).focus.clone().focus(window, cx));
    visibility(&mut cx, &host, true);
    press(&mut cx, "enter");
    assert_eq!(cx.update(|_, cx| host.read(cx).presses), 1);
    cx.update(|window, cx| {
        view.read(cx)
            .editor
            .clone()
            .update(cx, |state, cx| state.focus(window, cx))
    });
    let kept = editors.get(&a).unwrap();
    assert_eq!(kept.entity_id(), view.entity_id());
    assert!(cx.update(|_, cx| kept.read(cx).unsaved(cx)));
    assert_eq!(
        cx.update(|_, cx| kept.read(cx).editor.read(cx).selected_range()),
        3..5
    );
    assert_eq!(
        cx.update(|_, cx| kept.read(cx).editor.read(cx).scroll_offset().y),
        px(-90.)
    );
    undo(&mut cx);
    assert!(!cx.update(|_, cx| kept.read(cx).unsaved(cx)));
    editors.forget_file(&a);
    assert!(editors.get(&a).is_none());
    editors.keep(a.clone(), view.clone());
    editors.forget_repo(&WorkspaceId("w".into()), &RepoId("r".into()));
    assert!(editors.get(&a).is_none());
    editors.keep(a.clone(), view);
    editors.forget_workspace(&WorkspaceId("w".into()));
    assert!(editors.get(&a).is_none());
}

#[gpui::test]
fn arch_7_conflict_preparation_does_not_block_the_ui(cx: &mut TestAppContext) {
    let (mut cx, view) = editor(cx, buffer("", "old", ""));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.prepare(vec![chunk("stale", "x")], labels(), cx);
            assert!(view.prepared().is_none());
            view.prepare(vec![chunk("newest", "y")], labels(), cx);
            assert!(view.prepared().is_none());
        })
    });
    super::support::settle(&mut cx, |_, cx| view.read(cx).prepared().is_some());
    cx.update(|window, cx| view.update(cx, |view, cx| view.install_prepared(window, cx)));
    assert!(value(&mut cx, &view).contains("newest"));
    assert!(!value(&mut cx, &view).contains("stale"));
}

#[gpui::test]
fn ops_9_buffer_edits_never_write_or_stage(cx: &mut TestAppContext) {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .env("GIT_CONFIG_GLOBAL", dir.path().join("no-global-config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(git(&["init", "--initial-branch=base"]).status.success());
    for args in [
        ["config", "user.name", "Test"],
        ["config", "user.email", "test@example.com"],
        ["config", "core.autocrlf", "false"],
    ] {
        assert!(git(&args).status.success());
    }
    let path = dir.path().join("file");
    std::fs::write(&path, "base\n").unwrap();
    assert!(git(&["add", "."]).status.success());
    assert!(git(&["commit", "-m", "base"]).status.success());
    assert!(git(&["checkout", "-b", "side"]).status.success());
    std::fs::write(&path, "theirs\n").unwrap();
    git(&["commit", "-am", "theirs"]);
    git(&["checkout", "-b", "ours", "HEAD~1"]);
    std::fs::write(&path, "ours\n").unwrap();
    git(&["commit", "-am", "ours"]);
    assert!(!git(&["merge", "side"]).status.success());
    let disk = std::fs::read(&path).unwrap();
    let index = git(&["ls-files", "--stage"]).stdout;
    let (mut cx, view) = editor(cx, buffer("", "ours\n", ""));
    take(&mut cx, &view, 0, Take::Both);
    undo(&mut cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.accept(0, "accepted", window, cx)));
    cx.run_until_parked();
    edit(&mut cx, &view, 0..0, "manual");
    let mut editors = Editors::default();
    editors.keep(key("w", "r", "file"), view);
    assert_eq!(std::fs::read(path).unwrap(), disk);
    assert_eq!(git(&["ls-files", "--stage"]).stdout, index);
}

#[gpui::test]
fn ops_3_identical_adjacent_empty_takes_restore_distinct_ids(cx: &mut TestAppContext) {
    let b = Buffer::build(&[chunk("", ""), chunk("", "")], &labels());
    let (mut cx, view) = editor(cx, b);
    edit(&mut cx, &view, 0..0, "prefix\n");
    assert!(take(&mut cx, &view, 1, Take::Ours));
    assert!(take(&mut cx, &view, 0, Take::Ours));
    undo(&mut cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).ranges(cx))
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<Vec<_>>(),
        vec![0]
    );
    undo(&mut cx);
    let ranges = cx.update(|_, cx| view.read(cx).ranges(cx));
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].1.end, ranges[1].1.start);
}

#[gpui::test]
fn ops_3_empty_adjacent_takes_remain_separate_through_redo(cx: &mut TestAppContext) {
    for order in [[0, 1], [1, 0]] {
        let b = Buffer::build(&[chunk("", ""), chunk("", "")], &labels());
        let (mut cx, view) = editor(cx, b);
        edit(&mut cx, &view, 0..0, "🦀\n");
        for id in order {
            assert!(take(&mut cx, &view, id, Take::Both));
        }
        assert_eq!(value(&mut cx, &view), "🦀\n");
        undo(&mut cx);
        assert_eq!(
            cx.update(|_, cx| view.read(cx).ranges(cx))
                .iter()
                .map(|(id, _, _)| *id)
                .collect::<Vec<_>>(),
            vec![order[1]]
        );
        undo(&mut cx);
        let ranges = cx.update(|_, cx| view.read(cx).ranges(cx));
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].1.end, ranges[1].1.start);
        redo(&mut cx);
        redo(&mut cx);
        assert_eq!(value(&mut cx, &view), "🦀\n");
        assert!(cx.update(|_, cx| view.read(cx).ranges(cx)).is_empty());
    }
}

#[gpui::test]
fn arch_7_preparation_cannot_overwrite_a_subsequent_edit(cx: &mut TestAppContext) {
    let (mut cx, view) = editor(cx, buffer("", "old", ""));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.prepare(vec![chunk("stale", "x")], labels(), cx);
            view.editor.update(cx, |state, cx| {
                state.set_selected_range(0..0, cx);
                state.replace("manual", window, cx)
            });
        })
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| view.read(cx).prepared().is_none()));
    cx.update(|window, cx| view.update(cx, |view, cx| view.install_prepared(window, cx)));
    assert!(value(&mut cx, &view).starts_with("manual"));
}

#[gpui::test]
fn ops_3_manual_deletion_undo_cannot_restore_an_identical_taken_neighbor(cx: &mut TestAppContext) {
    let b = Buffer::build(&[chunk("", ""), chunk("", "")], &labels());
    let (mut cx, view) = editor(cx, b);
    edit(&mut cx, &view, 0..0, "prefix\n");
    assert!(take(&mut cx, &view, 1, Take::Ours));
    let range = cx.update(|_, cx| view.read(cx).ranges(cx)[0].1.clone());
    edit(&mut cx, &view, range, "");
    undo(&mut cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).ranges(cx))
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<Vec<_>>(),
        vec![0]
    );
    undo(&mut cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).ranges(cx)).len(), 2);
}

#[gpui::test]
fn ops_3_edit_immediately_after_undo_does_not_lose_restored_anchor(cx: &mut TestAppContext) {
    let b = buffer("head\n", "", "tail");
    let range = b.hunks[0].range.clone();
    let (mut cx, view) = editor(cx, b);
    assert!(take(&mut cx, &view, 0, Take::Ours));
    cx.update(|window, cx| {
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
            cx,
        );
        view.read(cx).editor.clone().update(cx, |state, cx| {
            state.set_selected_range(0..0, cx);
            state.replace("前", window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).ranges(cx)),
        vec![(0, range.start + 3..range.end + 3, true)]
    );
}
