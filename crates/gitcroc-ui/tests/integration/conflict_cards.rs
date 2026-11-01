//! M6-19b3: the base and AI proposal cards beside a hunk (AI-11, AI-12,
//! AI-16), with supplied core proposals and no AI provider.
use super::conflict_view::{
    Dock, Host, bounds_of, chunk, click, edit, editor, focus_text, focused, frame, has, hosted,
    hunk_range, labels, press, resolved, text_focused, undo, value,
};
use gitcroc_core::{FileProposal, ProposalConfidence, ProposalStatus, ProposedHunk};
use gitcroc_ui::conflict::{Buffer, ConflictEditor};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, point, px, size};

fn proposed(id: u32, text: Option<&str>, confidence: ProposalConfidence) -> ProposedHunk {
    ProposedHunk {
        id,
        status: if text.is_some() {
            ProposalStatus::Resolved
        } else {
            ProposalStatus::Unresolved
        },
        resolution: text.map(str::to_owned),
        explanation: format!("Explanation {id}"),
        confidence,
    }
}
fn proposal(hunks: Vec<ProposedHunk>) -> FileProposal {
    FileProposal {
        hunks,
        usage: Vec::new(),
    }
}
/// `count` hunks, each between two resolved lines.
fn hunks(count: usize) -> Buffer {
    let mut chunks = Vec::new();
    for n in 0..count {
        chunks.push(resolved(&format!("line {n}\n")));
        chunks.push(chunk(
            &format!("ours {n}\n"),
            &format!("base {n}\n"),
            &format!("theirs {n}\n"),
        ));
    }
    chunks.push(resolved("end\n"));
    Buffer::build(&chunks, &labels("main", "feature"))
}
fn supply(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>, given: Option<FileProposal>) {
    cx.update(|_, cx| view.update(cx, |view, cx| view.set_proposal(given, cx)));
    frame(cx);
}
fn in_card(cx: &mut VisualTestContext, hunk: usize, label: &str) -> bool {
    cx.update(|window, _| {
        window.try_find(format!("conflict-card-{hunk}")).is_some()
            && window
                .within(format!("conflict-card-{hunk}"))
                .try_find_by_label(label)
                .is_some()
    })
}
fn text_of(cx: &mut VisualTestContext, id: String) -> Option<String> {
    cx.update(|window, _| {
        window
            .try_find(id)
            .and_then(|e| e.value().map(str::to_owned))
    })
}
fn disabled(cx: &mut VisualTestContext, label: &str) -> bool {
    cx.update(|window, _| window.find_by_label(label).disabled() == Some(true))
}
fn accept_all(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>) -> bool {
    let done = cx.update(|window, cx| view.update(cx, |view, cx| view.accept_all_high(window, cx)));
    frame(cx);
    done
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
/// Shows another file's editor in the host's place and comes back, as
/// switching files does (OPS-9, UI-29).
fn switch_away_and_back(cx: &mut VisualTestContext, host: &Entity<Host>) {
    let other = cx.update(|window, cx| cx.new(|cx| ConflictEditor::new(hunks(1), window, cx)));
    for visible in [false, true] {
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
        frame(cx);
    }
    let kept = cx.update(|_, cx| host.read(cx).view.clone());
    for view in [other, kept] {
        cx.update(|_, cx| {
            host.update(cx, |host, cx| {
                host.view = view;
                cx.notify();
            })
        });
        frame(cx);
    }
}

#[gpui::test]
fn ai_11_cards_show_base_and_proposal_review(cx: &mut TestAppContext) {
    let (mut cx, host, view) = hosted(cx, hunks(3), Dock::Full);
    supply(
        &mut cx,
        &view,
        Some(proposal(vec![
            proposed(1, Some("merged 0\n"), ProposalConfidence::High),
            proposed(2, Some("merged 1\n"), ProposalConfidence::Medium),
            proposed(3, None, ProposalConfidence::Low),
        ])),
    );
    // The proposed text, the explanation and the confidence, in words.
    assert!(in_card(&mut cx, 0, "Proposal for conflict 1"));
    assert!(in_card(&mut cx, 0, "High confidence"));
    assert!(in_card(&mut cx, 1, "Medium confidence"));
    assert_eq!(
        text_of(&mut cx, "conflict-proposal-text-0".into()).as_deref(),
        Some("merged 0")
    );
    assert_eq!(
        text_of(&mut cx, "conflict-explanation-0".into()).as_deref(),
        Some("Explanation 1")
    );
    for label in [
        "Accept the proposal for conflict 1",
        "Reject the proposal for conflict 1",
        "Edit the proposal for conflict 1",
    ] {
        assert!(has(&mut cx, label), "{label}");
    }
    // An unresolved hunk: its explanation and Reject, nothing to accept.
    assert!(in_card(&mut cx, 2, "Not resolved"));
    assert!(in_card(&mut cx, 2, "Low confidence"));
    assert_eq!(
        text_of(&mut cx, "conflict-explanation-2".into()).as_deref(),
        Some("Explanation 3")
    );
    assert!(text_of(&mut cx, "conflict-proposal-text-2".into()).is_none());
    assert!(has(&mut cx, "Reject the proposal for conflict 3"));
    assert!(!has(&mut cx, "Accept the proposal for conflict 3"));
    assert!(!has(&mut cx, "Edit the proposal for conflict 3"));

    // Show base puts the base in the same card.
    click(&mut cx, "Show base of conflict 1");
    assert!(in_card(&mut cx, 0, "Base of conflict 1"));
    assert_eq!(
        text_of(&mut cx, "conflict-base-text-0".into()).as_deref(),
        Some("base 0")
    );

    // Reject hides the proposal and stays rejected across a file switch.
    click(&mut cx, "Reject the proposal for conflict 2");
    assert!(!has(&mut cx, "Proposal for conflict 2"));
    // Edit opens the text in an editor of its own, which keeps what is
    // typed across a file switch.
    click(&mut cx, "Edit the proposal for conflict 1");
    assert!(has(&mut cx, "Edited proposal for conflict 1"));
    cx.update(|window, cx| window.input("edited ", cx));
    frame(&mut cx);
    switch_away_and_back(&mut cx, &host);
    let view = cx.update(|_, cx| host.read(cx).view.clone());
    assert!(!has(&mut cx, "Proposal for conflict 2"));
    assert_eq!(
        cx.update(|_, cx| view.read(cx).edited_text(0, cx))
            .as_deref(),
        Some("edited merged 0\n")
    );
    // Escape cancels the edit; Edit again starts from the proposal.
    cx.update(|window, cx| {
        let editor = view.read(cx).proposal_editor(0).unwrap();
        editor.update(cx, |state, cx| state.focus(window, cx));
    });
    press(&mut cx, "escape");
    assert!(!has(&mut cx, "Edited proposal for conflict 1"));
    assert!(text_focused(&mut cx, &view));
    click(&mut cx, "Edit the proposal for conflict 1");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).edited_text(0, cx))
            .as_deref(),
        Some("merged 0\n")
    );
    // Ctrl+Enter (Cmd+Enter) accepts the edited text.
    cx.update(|window, cx| window.input("x", cx));
    press(
        &mut cx,
        if cfg!(target_os = "macos") {
            "cmd-enter"
        } else {
            "ctrl-enter"
        },
    );
    assert!(value(&mut cx, &view).starts_with("line 0\nxmerged 0\nline 1\n"));
    undo(&mut cx);

    // From the keyboard: the card's buttons follow the hunk's, and the
    // hunk keys act on the proposal at the cursor.
    focus_text(&mut cx, &view, 0);
    press(&mut cx, "f7");
    assert!(focused(&mut cx, "Take ours in conflict 1"));
    for _ in 0..3 {
        press(&mut cx, "tab");
    }
    assert!(focused(&mut cx, "Hide base of conflict 1"));
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "Accept the proposal for conflict 1"));
    press(&mut cx, "enter");
    assert!(value(&mut cx, &view).starts_with("line 0\nmerged 0\nline 1\n"));
    assert!(text_focused(&mut cx, &view));
    undo(&mut cx);
    let third = hunk_range(&mut cx, &view, 2);
    focus_text(&mut cx, &view, third.start + 1);
    press(&mut cx, "alt-4");
    assert!(value(&mut cx, &view).contains("<<<<<<< main\nours 2"));
    press(&mut cx, "alt-5");
    assert!(!has(&mut cx, "Proposal for conflict 3"));
    let first = hunk_range(&mut cx, &view, 0);
    focus_text(&mut cx, &view, first.start + 1);
    press(&mut cx, "alt-6");
    assert!(has(&mut cx, "Edited proposal for conflict 1"));
}

#[gpui::test]
fn ai_11_cards_fit_narrow_panes_and_keep_the_caret_visible(cx: &mut TestAppContext) {
    let long = format!("ours {}\n", "x".repeat(400));
    let b = Buffer::build(
        &[
            resolved("head\n"),
            chunk(&long, "base\n", "theirs\n"),
            resolved("end\n"),
        ],
        &labels("main", "feature"),
    );
    let (mut cx, _, view) = hosted(cx, b, Dock::Right);
    supply(
        &mut cx,
        &view,
        Some(proposal(vec![proposed(
            1,
            Some("merged\n"),
            ProposalConfidence::High,
        )])),
    );
    for width in [px(1200.), px(800.)] {
        cx.simulate_resize(size(width, px(700.)));
        frame(&mut cx);
        let pane = cx.update(|_, cx| view.read(cx).editor.read(cx).input_bounds());
        let card = cx.update(|window, _| window.find("conflict-card-0").bounds());
        // Inside the pane, and every control a usable target.
        assert!(
            card.left() >= pane.left() && card.right() <= pane.right(),
            "{width:?}"
        );
        for label in [
            "Accept the proposal for conflict 1",
            "Reject the proposal for conflict 1",
            "Edit the proposal for conflict 1",
        ] {
            let target = bounds_of(&mut cx, label).unwrap();
            assert!(
                target.size.height >= px(16.) && target.size.width >= px(24.),
                "{label}"
            );
            assert!(card.contains(&target.center()), "{label}");
        }
        // A caret at the end of the long line, under where the card would
        // be: the card moves off it.
        let end = hunk_range(&mut cx, &view, 0).start + "<<<<<<< main\n".len() + long.len() - 1;
        focus_text(&mut cx, &view, end);
        frame(&mut cx);
        let caret = cx.update(|_, cx| {
            view.read(cx)
                .editor
                .read(cx)
                .range_to_bounds(&(end..end))
                .unwrap()
        });
        let card = cx.update(|window, _| window.find("conflict-card-0").bounds());
        assert!(
            !card.intersects(&caret),
            "{width:?}: {card:?} covers the caret {caret:?}"
        );
    }
    // Pressing its controls works at the narrow width.
    click(&mut cx, "Accept the proposal for conflict 1");
    assert_eq!(value(&mut cx, &view), "head\nmerged\nend\n");
    let _ = point(px(0.), px(0.));
}

#[gpui::test]
fn ai_12_accept_and_accept_high_only_edit_this_file(cx: &mut TestAppContext) {
    use std::process::Command;
    // A real merge conflict on disk, with its index stages, under an
    // isolated git configuration.
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .env("GIT_CONFIG_GLOBAL", dir.path().join("no-global-config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", dir.path())
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
    assert!(git(&["commit", "-am", "theirs"]).status.success());
    assert!(git(&["checkout", "-b", "ours", "HEAD~1"]).status.success());
    std::fs::write(&path, "ours\n").unwrap();
    assert!(git(&["commit", "-am", "ours"]).status.success());
    assert!(!git(&["merge", "side"]).status.success());
    let disk = std::fs::read(&path).unwrap();
    let index = git(&["ls-files", "--stage"]).stdout;

    let (mut cx, view) = editor(cx, hunks(6));
    let other = cx.update(|window, cx| cx.new(|cx| ConflictEditor::new(hunks(6), window, cx)));
    let other_text = value(&mut cx, &other);
    let given = proposal(vec![
        proposed(1, Some("high 0\n"), ProposalConfidence::High),
        proposed(2, Some("medium 1\n"), ProposalConfidence::Medium),
        proposed(3, Some("low 2\n"), ProposalConfidence::Low),
        proposed(4, Some("high 3\n"), ProposalConfidence::High),
        proposed(5, Some("high 4\n"), ProposalConfidence::High),
        proposed(6, Some("high 5\n"), ProposalConfidence::High),
    ]);
    supply(&mut cx, &view, Some(given.clone()));
    supply(&mut cx, &other, Some(given));
    let original = value(&mut cx, &view);

    // Accept replaces the hunk, markers included, in one undo step.
    click(&mut cx, "Accept the proposal for conflict 2");
    assert!(value(&mut cx, &view).contains("line 1\nmedium 1\nline 2\n"));
    undo(&mut cx);
    assert_eq!(value(&mut cx, &view), original);
    redo(&mut cx);
    assert!(value(&mut cx, &view).contains("medium 1\n"));
    undo(&mut cx);

    // A hand-edited hunk keeps its proposal, which can no longer overwrite it.
    let fourth = hunk_range(&mut cx, &view, 3);
    let ours = fourth.start + "<<<<<<< main\n".len();
    edit(&mut cx, &view, ours..ours, "edited");
    assert!(in_card(&mut cx, 3, "Hunk changed"));
    assert!(disabled(&mut cx, "Accept the proposal for conflict 4"));
    assert!(disabled(&mut cx, "Edit the proposal for conflict 4"));
    assert!(
        !cx.update(|window, cx| view.update(cx, |view, cx| view.accept_proposal(3, window, cx)))
    );
    // A rejected proposal is not accepted either.
    click(&mut cx, "Reject the proposal for conflict 5");
    let edited = value(&mut cx, &view);

    // Accept all high-confidence: this file's untouched high hunks only,
    // in one undo step.
    assert!(accept_all(&mut cx, &view));
    let accepted = value(&mut cx, &view);
    assert!(accepted.contains("line 0\nhigh 0\nline 1\n"));
    assert!(accepted.contains("line 5\nhigh 5\nend\n"));
    for kept in ["ours 1\n", "ours 2\n", "<<<<<<< main\nedited", "ours 4\n"] {
        assert!(accepted.contains(kept), "{kept}");
    }
    assert!(!has(&mut cx, "Take ours in conflict 6"));
    assert!(has(&mut cx, "Take ours in conflict 1"));
    undo(&mut cx);
    assert_eq!(value(&mut cx, &view), edited);
    assert!(has(&mut cx, "Take ours in conflict 6"));
    redo(&mut cx);
    assert_eq!(value(&mut cx, &view), accepted);
    assert!(!accept_all(&mut cx, &view));

    // Nothing reached another file, the disk or the index.
    assert_eq!(value(&mut cx, &other), other_text);
    assert_eq!(std::fs::read(&path).unwrap(), disk);
    assert_eq!(git(&["ls-files", "--stage"]).stdout, index);
}

#[gpui::test]
fn ai_16_absent_or_disabled_proposals_have_no_controls(cx: &mut TestAppContext) {
    let (mut cx, view) = editor(cx, hunks(2));
    let none = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window.try_find("conflict-card-0").is_none()
                && window.try_find("conflict-card-1").is_none()
        }) && !has(cx, "Accept the proposal for conflict 1")
            && !has(cx, "Reject the proposal for conflict 1")
            && !has(cx, "Edit the proposal for conflict 1")
    };
    // Without proposal data: no cards, no proposal controls, nothing to accept.
    assert!(none(&mut cx));
    assert!(!accept_all(&mut cx, &view));
    let given = proposal(vec![
        proposed(1, Some("a\n"), ProposalConfidence::High),
        proposed(2, Some("b\n"), ProposalConfidence::High),
    ]);
    supply(&mut cx, &view, Some(given.clone()));
    click(&mut cx, "Reject the proposal for conflict 1");
    click(&mut cx, "Edit the proposal for conflict 2");
    // AI features off: the proposals are dropped with what was done to them.
    supply(&mut cx, &view, None);
    assert!(none(&mut cx));
    assert!(!has(&mut cx, "Edited proposal for conflict 2"));
    assert!(!accept_all(&mut cx, &view));
    supply(&mut cx, &view, Some(given));
    assert!(has(&mut cx, "Accept the proposal for conflict 1"));
    assert!(has(&mut cx, "Accept the proposal for conflict 2"));
    assert!(!has(&mut cx, "Edited proposal for conflict 2"));
}
