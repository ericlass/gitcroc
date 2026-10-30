//! M6-19b: the conflict editor's code colours (UI-24, UI-25), and its rows,
//! headers and hunk controls (OPS-3, OPS-4), placed from the editor's own
//! geometry (PERF-1) and reachable from the keyboard (UI-22).
use super::support::open;
use gitcroc_core::{CodeTheme, ConflictChunk, ConflictLabels, Settings};
use gitcroc_ui::conflict::{Buffer, ConflictEditor, Look};
use gitcroc_ui::theme::AppTheme;
use gitcroc_ui::theme::code_themes::{CODE_THEMES, Syntax, code_theme};
use gitcroc_ui::theme::contrast::{TEXT, contrast};
use gpui_kit::base::input::RopeExt;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AppContext, Bounds, Entity, FocusHandle, Focusable, Pixels, TestAppContext, VisualTestContext,
    Window, div, point, prelude::*, px, size,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Dock {
    Full,
    Right,
    Bottom,
}

/// The editor in a pane docked as the app docks the changes pane, beside
/// a control that can hold the focus.
pub(super) struct Host {
    pub view: Entity<ConflictEditor>,
    pub visible: bool,
    dock: Dock,
    pub other: FocusHandle,
    pub presses: usize,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let other = gpui_component::button::Button::new("other-control")
            .label("Other")
            .track_focus(&self.other)
            .on_click(cx.listener(|this, _, _, _| this.presses += 1));
        let pane = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .when(self.visible, |pane| pane.child(self.view.clone()));
        let side = div().flex_none().child(other);
        match self.dock {
            Dock::Full => div().size_full().flex().flex_col().child(side.h(px(40.))),
            Dock::Right => div().size_full().flex().flex_row().child(side.w(px(500.))),
            Dock::Bottom => div().size_full().flex().flex_col().child(side.h(px(300.))),
        }
        .child(pane)
    }
}

pub(super) fn labels(ours: &str, theirs: &str) -> ConflictLabels {
    ConflictLabels {
        ours: ours.into(),
        theirs: theirs.into(),
    }
}
pub(super) fn chunk(ours: &str, base: &str, theirs: &str) -> ConflictChunk {
    ConflictChunk::Hunk {
        ours: ours.into(),
        base: base.into(),
        theirs: theirs.into(),
    }
}
pub(super) fn resolved(text: &str) -> ConflictChunk {
    ConflictChunk::Resolved { text: text.into() }
}
/// A Rust file of `lines` lines with `hunks` hunks spread evenly, as the
/// spike generated it.
fn generated(lines: usize, hunks: usize) -> Buffer {
    let gap = lines / (hunks + 1);
    let mut chunks = Vec::new();
    let mut line = 0;
    for hunk in 0..hunks {
        let mut text = String::new();
        for _ in 0..gap.saturating_sub(5) {
            text.push_str(&format!("fn line_{line}() {{ let x = {line}; }}\n"));
            line += 1;
        }
        chunks.push(resolved(&text));
        chunks.push(chunk(
            &format!("let ours_{hunk} = 1;\n"),
            "let base = 0;\n",
            &format!("let theirs_{hunk} = 2;\n"),
        ));
    }
    let mut tail = String::new();
    for _ in line + 5 * hunks..lines {
        tail.push_str("fn end() {}\n");
    }
    chunks.push(resolved(&tail));
    Buffer::build(&chunks, &labels("main", "feature"))
}

pub(super) fn hosted(
    cx: &mut TestAppContext,
    buffer: Buffer,
    dock: Dock,
) -> (VisualTestContext, Entity<Host>, Entity<ConflictEditor>) {
    let (mut cx, host) = open(cx, |window, cx| {
        cx.new(|cx| Host {
            view: cx.new(|cx| ConflictEditor::new(buffer, window, cx)),
            visible: true,
            dock,
            other: cx.focus_handle(),
            presses: 0,
        })
    });
    let view = cx.update(|_, cx| host.read(cx).view.clone());
    frame(&mut cx);
    (cx, host, view)
}
pub(super) fn editor(
    cx: &mut TestAppContext,
    buffer: Buffer,
) -> (VisualTestContext, Entity<ConflictEditor>) {
    let (cx, _, view) = hosted(cx, buffer, Dock::Full);
    (cx, view)
}
/// Settles the editor's work and draws until its geometry is stable.
pub(super) fn frame(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
}
pub(super) fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|window, cx| window.press(key, cx));
    frame(cx);
}
pub(super) fn value(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>) -> String {
    cx.update(|_, cx| view.read(cx).value(cx))
}
pub(super) fn edit(
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
    frame(cx);
}
pub(super) fn undo(cx: &mut VisualTestContext) {
    press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-z"
        } else {
            "ctrl-z"
        },
    );
}
pub(super) fn focus_text(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>, at: usize) {
    cx.update(|window, cx| {
        view.read(cx).editor.clone().update(cx, |state, cx| {
            state.set_selected_range(at..at, cx);
            state.focus(window, cx);
        })
    });
    frame(cx);
}
fn scroll_to(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>, y: Pixels) {
    cx.update(|_, cx| {
        view.read(cx).editor.clone().update(cx, |state, cx| {
            state.set_scroll_offset(point(px(0.), -y), cx)
        })
    });
    frame(cx);
}
pub(super) fn bounds_of(cx: &mut VisualTestContext, label: &str) -> Option<Bounds<Pixels>> {
    cx.update(|window, _| window.try_find_by_label(label).map(|e| e.bounds()))
}
/// Clicks the control named `label` at its centre, as a pointer would.
pub(super) fn click(cx: &mut VisualTestContext, label: &str) {
    cx.update(|window, cx| {
        let id = window.find_by_label(label).path().last().cloned().unwrap();
        window.click(id, cx);
    });
    frame(cx);
}
pub(super) fn has(cx: &mut VisualTestContext, label: &str) -> bool {
    bounds_of(cx, label).is_some()
}
pub(super) fn focused(cx: &mut VisualTestContext, label: &str) -> bool {
    cx.update(|window, _| {
        window
            .try_find_by_label(label)
            .is_some_and(|e| e.focused() == Some(true))
    })
}
pub(super) fn text_focused(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>) -> bool {
    cx.update(|window, cx| view.read(cx).editor.focus_handle(cx).is_focused(window))
}
/// Where the editor draws the line that starts at `offset`.
fn line_bounds(
    cx: &mut VisualTestContext,
    view: &Entity<ConflictEditor>,
    offset: usize,
) -> Option<Bounds<Pixels>> {
    cx.update(|_, cx| {
        view.read(cx)
            .editor
            .read(cx)
            .range_to_bounds(&(offset..offset))
    })
}
fn line_height(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>) -> Pixels {
    cx.update(|_, cx| view.read(cx).editor.read(cx).line_height().unwrap())
}
pub(super) fn hunk_range(
    cx: &mut VisualTestContext,
    view: &Entity<ConflictEditor>,
    id: usize,
) -> std::ops::Range<usize> {
    cx.update(|_, cx| {
        view.read(cx)
            .ranges(cx)
            .into_iter()
            .find(|(at, _, _)| *at == id)
            .map(|(_, range, _)| range)
            .unwrap()
    })
}
fn close(a: Pixels, b: Pixels) -> bool {
    (f32::from(a) - f32::from(b)).abs() < 0.5
}

#[gpui::test]
fn ops_4_headers_keep_core_labels_for_merge_and_rebase(cx: &mut TestAppContext) {
    // The core names the sides (gitcroc-git `conflict_labels`): in a merge,
    // ours is the checked-out branch and theirs the merged one; in a rebase
    // ours is what the rebase builds on and theirs the replayed commit.
    for (ours, theirs) in [
        ("main", "feature/login"),
        ("origin/main", "1a2b3c4 Add login (feature/login)"),
    ] {
        let b = Buffer::build(
            &[resolved("head\n"), chunk("a\n", "", "b\n")],
            &labels(ours, theirs),
        );
        let (mut cx, _) = editor(cx, b);
        assert!(has(&mut cx, &format!("Ours: {ours}")), "{ours}");
        assert!(has(&mut cx, &format!("Theirs: {theirs}")), "{theirs}");
        assert!(has(&mut cx, "End of conflict 1"));
        // Never the generic words in place of the label.
        assert!(!has(&mut cx, "Ours: ours"));
    }
}

#[gpui::test]
fn ops_3_hunk_buttons_follow_anchors_and_manual_edits(cx: &mut TestAppContext) {
    // A resolved line that looks like a marker is text, not a hunk (OPS-8).
    let b = Buffer::build(
        &[
            resolved("<<<<<<< not a hunk\n"),
            chunk("a\n", "base\n", "b\n"),
            resolved("mid\n"),
            chunk("c\n", "", "d\n"),
        ],
        &labels("main", "feature"),
    );
    let (mut cx, view) = editor(cx, b);
    for n in [1, 2] {
        for name in ["Take ours", "Take theirs", "Take both"] {
            let label = format!("{name} in conflict {n}");
            assert!(has(&mut cx, &label), "{label}");
        }
    }
    assert!(!has(&mut cx, "Take ours in conflict 3"));

    // An edit above moves the controls with the anchor, a line down.
    let before = bounds_of(&mut cx, "Take ours in conflict 1").unwrap();
    let height = line_height(&mut cx, &view);
    edit(&mut cx, &view, 0..0, "added\n");
    let after = bounds_of(&mut cx, "Take ours in conflict 1").unwrap();
    assert!(close(after.origin.y, before.origin.y + height));

    // A button calls the take; the rest renumber, from the anchors.
    click(&mut cx, "Take theirs in conflict 1");
    assert!(value(&mut cx, &view).starts_with("added\n<<<<<<< not a hunk\nb\nmid\n"));
    assert!(has(&mut cx, "Take ours in conflict 1"));
    assert!(!has(&mut cx, "Take ours in conflict 2"));
    undo(&mut cx);
    assert!(has(&mut cx, "Take ours in conflict 2"));

    // A hand-edited hunk keeps its highlight but loses its actions.
    let second = hunk_range(&mut cx, &view, 1);
    edit(&mut cx, &view, second.start + 1..second.start + 2, "x");
    assert!(!has(&mut cx, "Take ours in conflict 2"));
    assert_eq!(
        cx.update(|window, _| window.find_all_by_label("Ours: main").len()),
        1
    );
    assert!(cx.update(|window, _| window.try_find("conflict-changed-1").is_some()));
    assert!(has(&mut cx, "Take ours in conflict 1"));
    undo(&mut cx);
    assert!(has(&mut cx, "Take ours in conflict 2"));
    assert!(cx.update(|window, _| window.try_find("conflict-changed-1").is_none()));
}

/// Each overlay of hunk `id` against the editor's own line bounds.
fn assert_overlays_match(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>, id: usize) {
    let range = hunk_range(cx, view, id);
    let height = line_height(cx, view);
    let (head, separator, end) = cx.update(|_, cx| view.read(cx).marker_offsets(id, cx).unwrap());
    assert_eq!(head, range.start);
    let n = cx.update(|_, cx| {
        view.read(cx)
            .ranges(cx)
            .iter()
            .position(|(at, _, _)| *at == id)
            .unwrap()
            + 1
    });
    for (offset, element) in [
        (head, format!("conflict-ours-{id}")),
        (separator, format!("conflict-theirs-{id}")),
        (end, format!("conflict-end-{id}")),
    ] {
        let line = line_bounds(cx, view, offset).unwrap();
        let header = cx.update(|window, _| window.find(element.clone()).bounds());
        assert!(close(header.origin.y, line.origin.y), "{element}");
        assert!(close(header.size.height, height), "{element}");
    }
    let rows = cx.update(|window, _| window.find(format!("conflict-rows-{id}-ours")).bounds());
    let ours_line = line_bounds(cx, view, head).unwrap().origin.y + height;
    assert!(close(rows.origin.y, ours_line));
    let take = bounds_of(cx, &format!("Take ours in conflict {n}")).unwrap();
    let line = line_bounds(cx, view, head).unwrap();
    assert!(take.origin.y >= line.origin.y - px(0.5));
    assert!(take.origin.y + take.size.height <= line.origin.y + height + px(0.5));
}

#[gpui::test]
fn ops_3_overlay_bounds_match_editor_ranges(cx: &mut TestAppContext) {
    for dock in [Dock::Full, Dock::Right, Dock::Bottom] {
        let (mut cx, _, view) = hosted(cx, generated(600, 6), dock);
        // First, middle and last visible hunk after scrolling to each.
        let height = line_height(&mut cx, &view);
        let ids = [0, 2, 5];
        for id in ids {
            let range = hunk_range(&mut cx, &view, id);
            let line = cx.update(|_, cx| {
                view.read(cx)
                    .editor
                    .read(cx)
                    .text()
                    .offset_to_point(range.start)
                    .row
            });
            scroll_to(&mut cx, &view, height * (line.saturating_sub(3)) as f32);
            assert_overlays_match(&mut cx, &view, id);
        }
        // A changed code font size changes the rows; they follow (UI-26).
        cx.update(|_, cx| {
            let mut settings = Settings::default();
            settings.appearance.code_font_size = 20;
            AppTheme::apply_settings(&settings, cx);
        });
        frame(&mut cx);
        let larger = line_height(&mut cx, &view);
        assert!(larger > height, "{dock:?}");
        let range = hunk_range(&mut cx, &view, 5);
        let row = cx.update(|_, cx| {
            view.read(cx)
                .editor
                .read(cx)
                .text()
                .offset_to_point(range.start)
                .row
        });
        scroll_to(&mut cx, &view, larger * (row - 3) as f32);
        assert_overlays_match(&mut cx, &view, 5);
        // A resized window.
        cx.simulate_resize(size(px(900.), px(600.)));
        frame(&mut cx);
        assert_overlays_match(&mut cx, &view, 5);
        cx.update(|_, cx| AppTheme::apply_settings(&Settings::default(), cx));
        frame(&mut cx);

        // A header half above the viewport is clipped: what is outside it
        // is neither drawn over the pane above nor clickable.
        let range = hunk_range(&mut cx, &view, 2);
        let line_height = line_height(&mut cx, &view);
        let row = cx.update(|_, cx| {
            view.read(cx)
                .editor
                .read(cx)
                .text()
                .offset_to_point(range.start)
                .row
        });
        // Within the editor's own padding, still inside this view.
        scroll_to(
            &mut cx,
            &view,
            line_height * row as f32 + line_height * 0.35,
        );
        let viewport = cx.update(|_, cx| view.read(cx).editor.read(cx).input_bounds());
        let take = cx.update(|window, _| window.find("conflict-2-take-ours"));
        assert!(
            take.bounds().origin.y + px(2.) < viewport.origin.y,
            "{dock:?}"
        );
        assert!(take.bounds().bottom() > viewport.origin.y, "{dock:?}");
        let original = value(&mut cx, &view);
        // Its part above the viewport is not a hit target.
        cx.update(|window, cx| window.click_at("conflict-2-take-ours", point(px(4.), px(2.)), cx));
        frame(&mut cx);
        assert_eq!(value(&mut cx, &view), original, "{dock:?}");
        // Nothing of a hunk below the viewport is laid out.
        assert!(cx.update(|window, _| window.try_find("conflict-ours-5").is_none()));
    }
}

#[gpui::test]
fn ui_24_conflict_syntax_and_theme_changes(cx: &mut TestAppContext) {
    let b = Buffer::build(
        &[
            resolved("fn main() {\n"),
            chunk("    let s = \"ours\";\n", "", "    // theirs\n"),
            resolved("}\n"),
        ],
        &labels("main", "feature"),
    );
    let (mut cx, view) = editor(cx, b);
    // Before a language is known, and for one syntect does not know, the
    // text is plain in the code theme's text colour.
    assert_eq!(cx.update(|_, cx| view.read(cx).syntax_at(0, cx)), None);
    cx.update(|_, cx| view.update(cx, |view, cx| view.set_language("notes.unknown-type", cx)));
    frame(&mut cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).syntax_at(0, cx)), None);
    let entity = cx.update(|_, cx| view.read(cx).editor.entity_id());
    let text = value(&mut cx, &view);

    cx.update(|_, cx| view.update(cx, |view, cx| view.set_language("src/main.rs", cx)));
    frame(&mut cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).syntax_at(0, cx)),
        Some(Syntax::Keyword)
    );
    let s = text.find("\"ours\"").unwrap();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).syntax_at(s, cx)),
        Some(Syntax::String)
    );
    let comment = text.find("// theirs").unwrap();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).syntax_at(comment, cx)),
        Some(Syntax::Comment)
    );

    for theme in [CodeTheme::SolarizedLight, CodeTheme::Dracula] {
        cx.update(|_, cx| {
            let mut settings = Settings::default();
            settings.appearance.code_theme_light = theme;
            AppTheme::apply_settings(&settings, cx);
        });
        frame(&mut cx);
        let colors = code_theme(theme).colors;
        let look = cx.update(|_, cx| view.read(cx).look(cx));
        assert_eq!(look, Look::of(&colors));
        assert_eq!(look.syntax(Syntax::Keyword), colors.keyword);
        assert_eq!(look.rows, colors.conflict);
        assert_eq!(look.text, colors.text);
        // The editor is recoloured, not replaced: same text, same undo.
        assert_eq!(cx.update(|_, cx| view.read(cx).editor.entity_id()), entity);
        assert_eq!(value(&mut cx, &view), text);
        assert_eq!(
            cx.update(|_, cx| view.read(cx).syntax_at(0, cx)),
            Some(Syntax::Keyword)
        );
    }
    edit(&mut cx, &view, 0..0, "// top\n");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).syntax_at(0, cx)),
        Some(Syntax::Comment)
    );
    undo(&mut cx);
    assert_eq!(value(&mut cx, &view), text);
}

#[test]
fn ui_25_conflict_text_contrast_meets_four_point_five() {
    let mut failures = Vec::new();
    for theme in CODE_THEMES {
        let def = code_theme(theme);
        for (pair, front, back) in Look::of(&def.colors).text_pairs() {
            let ratio = contrast(front, back);
            if ratio < TEXT {
                failures.push(format!("{} {pair}: {ratio:.2}", def.label));
            }
        }
    }
    assert_eq!(failures, Vec::<String>::new());
}

#[gpui::test]
fn perf_1_conflict_overlays_only_lay_out_visible_rows(cx: &mut TestAppContext) {
    let b = generated(10_000, 50);
    let lines = b.text.lines().count();
    assert!(lines >= 10_000, "{lines}");
    assert_eq!(b.hunks.len(), 50);
    let (mut cx, view) = editor(cx, b);
    let count = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            (0..50)
                .filter(|id| {
                    window.try_find(format!("conflict-ours-{id}")).is_some()
                        || window.try_find(format!("conflict-changed-{id}")).is_some()
                })
                .count()
        })
    };
    // About 200 lines between hunks: a screen shows at most two.
    assert!(count(&mut cx) <= 2);
    assert!(cx.update(|_, cx| view.read(cx).laid_out()) <= 2);
    let height = line_height(&mut cx, &view);
    for hunk in [0, 12, 25, 37, 49] {
        let range = hunk_range(&mut cx, &view, hunk);
        let row = cx.update(|_, cx| {
            view.read(cx)
                .editor
                .read(cx)
                .text()
                .offset_to_point(range.start)
                .row
        });
        scroll_to(&mut cx, &view, height * row.saturating_sub(5) as f32);
        assert_eq!(count(&mut cx), 1, "{hunk}");
        assert!(cx.update(|window, _| window.try_find(format!("conflict-ours-{hunk}")).is_some()));
        assert_eq!(cx.update(|_, cx| view.read(cx).laid_out()), 1, "{hunk}");
    }
    for line in [2_500, 5_000, lines] {
        scroll_to(&mut cx, &view, height * line as f32);
        assert!(count(&mut cx) <= 2, "{line}");
        assert!(cx.update(|_, cx| view.read(cx).laid_out()) <= 2);
    }
}

#[gpui::test]
fn ui_22_hunk_controls_are_keyboard_reachable(cx: &mut TestAppContext) {
    let b = Buffer::build(
        &[
            chunk("a\n", "", "b\n"),
            resolved(&"line\n".repeat(200)),
            chunk("c\n", "", "d\n"),
        ],
        &labels("main", "feature"),
    );
    let (mut cx, host, view) = hosted(cx, b, Dock::Full);
    focus_text(&mut cx, &view, 0);
    // F7 goes to the next hunk's first button, revealing it.
    press(&mut cx, "f7");
    assert!(focused(&mut cx, "Take ours in conflict 2"));
    // Escape goes back to the text, at that hunk.
    press(&mut cx, "escape");
    assert!(text_focused(&mut cx, &view));
    let second = hunk_range(&mut cx, &view, 1);
    assert!(second.contains(&cx.update(|_, cx| view.read(cx).editor.read(cx).cursor())));
    // Shift+F7 from the text, and from a button, wrapping.
    press(&mut cx, "shift-f7");
    assert!(focused(&mut cx, "Take ours in conflict 1"));
    press(&mut cx, "shift-f7");
    assert!(focused(&mut cx, "Take ours in conflict 2"));
    // Tab goes on to the hunk's other buttons; Space and Enter press them,
    // and the editor's keys do not take them.
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "Take theirs in conflict 2"));
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "Take both in conflict 2"));
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "Show base of conflict 2"));
    press(&mut cx, "space");
    assert!(has(&mut cx, "Base of conflict 2"));
    assert!(focused(&mut cx, "Hide base of conflict 2"));
    press(&mut cx, "enter");
    assert!(!has(&mut cx, "Base of conflict 2"));
    assert!(focused(&mut cx, "Show base of conflict 2"));
    press(&mut cx, "shift-tab");
    assert!(focused(&mut cx, "Take both in conflict 2"));
    let original = value(&mut cx, &view);
    press(&mut cx, "space");
    assert!(value(&mut cx, &view).ends_with("c\nd\n"));
    assert!(text_focused(&mut cx, &view));
    undo(&mut cx);
    assert_eq!(value(&mut cx, &view), original);
    press(&mut cx, "f7");
    assert!(focused(&mut cx, "Take ours in conflict 1"));
    press(&mut cx, "tab");
    press(&mut cx, "enter");
    assert_eq!(
        value(&mut cx, &view),
        original.replacen(&original[..original.find("line\n").unwrap()], "b\n", 1,)
    );
    // The pressed button left with its hunk: the text has the focus.
    assert!(text_focused(&mut cx, &view));
    undo(&mut cx);
    assert_eq!(value(&mut cx, &view), original);
    // Another control keeps Enter and Space.
    cx.update(|window, cx| host.read(cx).other.clone().focus(window, cx));
    press(&mut cx, "enter");
    press(&mut cx, "space");
    assert_eq!(cx.update(|_, cx| host.read(cx).presses), 2);
    assert_eq!(value(&mut cx, &view), original);
}

#[gpui::test]
fn sec_3_conflict_labels_and_proposals_are_literal_text(cx: &mut TestAppContext) {
    let ours = "<b>main</b> & {x} \u{202e}rtl";
    let theirs = "$(rm -rf /) `id` \\n";
    let b = Buffer::build(
        &[chunk("<script>\n", "<i>base</i>\n", "b\n")],
        &labels(ours, theirs),
    );
    let (mut cx, view) = editor(cx, b);
    assert!(has(&mut cx, &format!("Ours: {ours}")));
    assert!(has(&mut cx, &format!("Theirs: {theirs}")));
    click(&mut cx, "Show base of conflict 1");
    // The model's words too: shown as given, never as markup (AI-11).
    let resolution = "<img src=x onerror=alert(1)>\n\u{202e}$(id)";
    let explanation = "**bold** <a href=\"https://example.com\">link</a>";
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_proposal(
                Some(gitcroc_core::FileProposal {
                    hunks: vec![gitcroc_core::ProposedHunk {
                        id: 1,
                        status: gitcroc_core::ProposalStatus::Resolved,
                        resolution: Some(resolution.into()),
                        explanation: explanation.into(),
                        confidence: gitcroc_core::ProposalConfidence::High,
                    }],
                    usage: Vec::new(),
                }),
                cx,
            )
        })
    });
    frame(&mut cx);
    let text = |cx: &mut VisualTestContext, id: &'static str| {
        cx.update(|window, _| window.find(id).value().map(str::to_owned))
    };
    assert_eq!(
        text(&mut cx, "conflict-base-text-0").as_deref(),
        Some("<i>base</i>")
    );
    assert_eq!(
        text(&mut cx, "conflict-proposal-text-0").as_deref(),
        Some(resolution.trim_end_matches('\n'))
    );
    assert_eq!(
        text(&mut cx, "conflict-explanation-0").as_deref(),
        Some(explanation)
    );
}

/// The spike's slow first frame at the document end (M6-02): scrolls the
/// 10,000-line, 50-hunk buffer 100 px a frame in a 1920×1003 window and
/// compares the first frame that shows its last line with the median. The
/// test platform has no GPU and no real text shaping, so this checks the
/// editor's and the overlays' layout only. Timing: `just timing`.
#[gpui::test]
#[ignore = "asserts times: run with `just timing` on the reference machine"]
fn perf_1_conflict_first_frame_at_the_end_is_not_slow(cx: &mut TestAppContext) {
    let (mut cx, view) = editor(cx, generated(10_000, 50));
    cx.simulate_resize(size(px(1920.), px(1003.)));
    frame(&mut cx);
    let mut frames = Vec::new();
    for step in 1..=2_100 {
        let y = px(100.) * step as f32;
        cx.update(|_, cx| {
            view.read(cx).editor.clone().update(cx, |state, cx| {
                state.set_scroll_offset(point(px(0.), -y), cx)
            })
        });
        // Two frames: the editor applies a set scroll in the next layout.
        let start = std::time::Instant::now();
        cx.update(|window, cx| window.render_frame(cx));
        cx.update(|window, cx| window.render_frame(cx));
        let end = cx.update(|_, cx| view.read(cx).editor.read(cx).visible_row_range());
        frames.push((start.elapsed(), end.is_some_and(|rows| rows.end >= 10_000)));
    }
    let mut sorted: Vec<_> = frames.iter().map(|(time, _)| *time).collect();
    sorted.sort();
    let median = sorted[sorted.len() / 2];
    let (at_end, _) = frames.iter().find(|(_, end)| *end).copied().unwrap();
    eprintln!("median {median:?}, first at the end {at_end:?}");
    assert!(at_end < median * 3, "{at_end:?} against {median:?}");
}
