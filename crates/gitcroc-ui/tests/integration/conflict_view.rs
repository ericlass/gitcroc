//! M6-19b: the conflict editor in the code theme's colours and syntax
//! (UI-24, UI-25).
use super::support::open;
use gitcroc_core::{CodeTheme, ConflictChunk, ConflictLabels, Settings};
use gitcroc_ui::conflict::{Buffer, ConflictEditor, Look};
use gitcroc_ui::theme::AppTheme;
use gitcroc_ui::theme::code_themes::{CODE_THEMES, Syntax, code_theme};
use gitcroc_ui::theme::contrast::{TEXT, contrast};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext};

fn labels(ours: &str, theirs: &str) -> ConflictLabels {
    ConflictLabels {
        ours: ours.into(),
        theirs: theirs.into(),
    }
}
fn chunk(ours: &str, base: &str, theirs: &str) -> ConflictChunk {
    ConflictChunk::Hunk {
        ours: ours.into(),
        base: base.into(),
        theirs: theirs.into(),
    }
}
fn resolved(text: &str) -> ConflictChunk {
    ConflictChunk::Resolved { text: text.into() }
}
/// Settles the editor's work and draws until its geometry is stable.
fn frame(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
}
fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|window, cx| window.press(key, cx));
    frame(cx);
}
fn value(cx: &mut VisualTestContext, view: &Entity<ConflictEditor>) -> String {
    cx.update(|_, cx| view.read(cx).value(cx))
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
    frame(cx);
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
fn editor(cx: &mut TestAppContext, buffer: Buffer) -> (VisualTestContext, Entity<ConflictEditor>) {
    let (mut cx, view) = open(cx, |window, cx| {
        cx.new(|cx| ConflictEditor::new(buffer, window, cx))
    });
    frame(&mut cx);
    (cx, view)
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
