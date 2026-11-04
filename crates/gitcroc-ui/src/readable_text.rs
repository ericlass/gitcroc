//! A text field a screen reader can read back (UI-22, M6-25c).
//! gpui-component's field exposes its name and value, not its text and
//! caret, so a screen reader could type into it but not review what it
//! typed. [`line`] and [`lines`] wrap a field in a node that exposes both, as
//! GPUI's accessibility guide shows: a text input whose synthetic children are
//! AccessKit text runs, one per line, and which holds the caret and the
//! selection. The field's own node gives way to it, and the wrapper tracks
//! the field's focus handle, outside the tab order, so that the screen
//! reader's focus lands on it. The commit message uses it; the other fields
//! keep their name and value (LATER).

use std::ops::Range;

use gpui_component::RoleOverride;
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::accesskit::{self, TextPosition, TextSelection};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    A11ySubtreeBuilder, AccessibleAction, App, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled,
    TestSupportExt, div,
};

/// The field a wrapper reads: a single line or a text area.
#[derive(Clone)]
pub enum Field {
    Line(Entity<InputState>),
    Lines(Entity<TextareaState>),
}

impl Field {
    fn role(&self) -> Role {
        match self {
            Self::Line(_) => Role::TextInput,
            Self::Lines(_) => Role::MultilineTextInput,
        }
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Line(state) => state.focus_handle(cx),
            Self::Lines(state) => state.focus_handle(cx),
        }
    }

    fn editable(&self, cx: &App) -> bool {
        match self {
            Self::Line(state) => state.read(cx).is_editable(),
            Self::Lines(state) => state.read(cx).is_editable(),
        }
    }

    /// The text, the selection and the caret, in bytes.
    fn read(&self, cx: &App) -> (SharedString, Range<usize>, usize) {
        match self {
            Self::Line(state) => {
                let state = state.read(cx);
                (state.value(), state.selected_range(), state.cursor())
            }
            Self::Lines(state) => {
                let state = state.read(cx);
                (state.value(), state.selected_range(), state.cursor())
            }
        }
    }

    fn set(&self, value: String, window: &mut gpui_kit::Window, cx: &mut App) {
        match self {
            Self::Line(state) => state.update(cx, |state, cx| state.set_value(value, window, cx)),
            Self::Lines(state) => state.update(cx, |state, cx| state.set_value(value, window, cx)),
        }
    }
}

/// A place in the text: a line, and a character in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub line: usize,
    pub character: usize,
}

/// What the wrapper tells a screen reader: the lines, each ending with its
/// line break, and where the selection starts and its caret is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runs {
    pub lines: Vec<String>,
    pub anchor: Place,
    pub caret: Place,
}

impl Runs {
    /// The runs of `text`, with `selection` and the `caret` at one of its
    /// ends, both in bytes.
    pub fn new(text: &str, selection: Range<usize>, caret: usize) -> Self {
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
        // After a final line break, or in an empty field, the caret has a
        // line of its own.
        if text.is_empty() || text.ends_with('\n') {
            lines.push(String::new());
        }
        let anchor = if caret == selection.start {
            selection.end
        } else {
            selection.start
        };
        Self {
            anchor: place(&lines, anchor),
            caret: place(&lines, caret),
            lines,
        }
    }

    /// The runs of `field` as it is now.
    pub fn of(field: &Field, cx: &App) -> Self {
        let (text, selection, caret) = field.read(cx);
        Self::new(&text, selection, caret)
    }

    /// The text run of each line.
    pub fn nodes(&self) -> Vec<accesskit::Node> {
        self.lines
            .iter()
            .map(|line| {
                let mut run = accesskit::Node::new(Role::TextRun);
                run.set_value(line.as_str());
                run.set_character_lengths(
                    line.chars()
                        .map(|c| u8::try_from(c.len_utf8()).unwrap_or(u8::MAX))
                        .collect::<Vec<_>>(),
                );
                run
            })
            .collect()
    }

    /// Adds the runs under the wrapper's node, and the selection to it.
    fn write(&self, builder: &mut A11ySubtreeBuilder) {
        for (index, run) in self.nodes().into_iter().enumerate() {
            builder.push_child(builder.synthetic_node_id(index), run);
        }
        let position = |place: Place| TextPosition {
            node: builder.synthetic_node_id(place.line),
            character_index: place.character,
        };
        let selection = TextSelection {
            anchor: position(self.anchor),
            focus: position(self.caret),
        };
        builder.parent_node().set_text_selection(selection);
    }
}

/// Where byte `offset` falls among `lines`. An offset just after a line
/// break starts the next line.
fn place(lines: &[String], offset: usize) -> Place {
    let mut start = 0;
    for (line, text) in lines.iter().enumerate() {
        let end = start + text.len();
        if offset < end || line + 1 == lines.len() {
            let within = offset.saturating_sub(start);
            let character = text
                .char_indices()
                .take_while(|(at, _)| *at < within)
                .count();
            return Place { line, character };
        }
        start = end;
    }
    Place {
        line: 0,
        character: 0,
    }
}

/// `handle` for a second element that tracks it, outside the tab order, so
/// that Tab stops on the field once. `tab_stop` also writes the flag the
/// window hands out with the focused handle; the field's stays as it was.
fn untabbable(handle: &FocusHandle) -> FocusHandle {
    let quiet = handle.clone().tab_stop(false);
    let _ = handle.clone().tab_stop(true);
    quiet
}

/// A single-line `input` of `state`, readable.
pub fn line(
    id: &'static str,
    input: Input,
    state: &Entity<InputState>,
    label: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    readable(
        id,
        input.role(RoleOverride::Presentational),
        Field::Line(state.clone()),
        label.into(),
        cx,
    )
}

/// A text `area` of `state`, readable. `wrap` puts the area in what holds
/// it, as [`crate::text_area::tab_moves_on`] does.
pub fn lines<E: IntoElement>(
    id: &'static str,
    area: Textarea,
    wrap: impl FnOnce(Textarea) -> E,
    state: &Entity<TextareaState>,
    label: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    readable(
        id,
        wrap(area.role(RoleOverride::Presentational)),
        Field::Lines(state.clone()),
        label.into(),
        cx,
    )
}

/// `field`, drawn by `child`, under a node that a screen reader reads its
/// text and caret from. The node takes the field's focus, its value, and
/// the actions a screen reader sends it.
fn readable(
    id: &'static str,
    child: impl IntoElement,
    field: Field,
    label: SharedString,
    cx: &App,
) -> impl IntoElement {
    let focus = field.focus_handle(cx);
    let (text, selection, caret) = field.read(cx);
    let editable = field.editable(cx);
    div()
        .id(id)
        .test_support()
        // A text area fills what holds it; a line takes its own height.
        .map(|this| match &field {
            Field::Line(_) => this.w_full(),
            Field::Lines(_) => this.size_full(),
        })
        .role(field.role())
        .aria_label(label)
        .aria_value(text.clone())
        .track_focus(&untabbable(&focus))
        .on_a11y_action(AccessibleAction::Focus, move |_, window, cx| {
            window.focus(&focus, cx);
        })
        .when(editable, |this| {
            let field = field.clone();
            this.on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                if let Some(accesskit::ActionData::Value(value)) = data {
                    field.set(value.to_string(), window, cx);
                }
            })
        })
        // Built only while assistive technology is active, after prepaint.
        .a11y_synthetic_children(move |builder| {
            Runs::new(&text, selection, caret).write(builder);
        })
        .child(child)
}

#[cfg(test)]
mod tests {
    // Test code (QA-4).
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::time::{Duration, Instant};

    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, ClipboardItem, Context, Render, Styled, TestAppContext, VisualTestContext,
        Window,
    };

    use super::*;

    /// A line and a text area, each wrapped, as the commit box has them.
    struct Probe {
        subject: Entity<InputState>,
        body: Entity<TextareaState>,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(line(
                    "subject",
                    Input::new(&self.subject),
                    &self.subject,
                    "Subject",
                    cx,
                ))
                .child(div().h_40().child(lines(
                    "body",
                    Textarea::new(&self.body).h(gpui_kit::relative(1.)),
                    |area| crate::text_area::tab_moves_on(area).size_full(),
                    &self.body,
                    "Body",
                    cx,
                )))
        }
    }

    fn probe(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
        cx.update(crate::init);
        cx.add_window_view(|window, cx| Probe {
            subject: cx.new(|cx| InputState::new(window, cx)),
            body: cx.new(|cx| TextareaState::new(window, cx)),
        })
    }

    fn runs(cx: &mut VisualTestContext, field: &Field) -> Runs {
        cx.run_until_parked();
        cx.update(|_, cx| Runs::of(field, cx))
    }

    fn at(line: usize, character: usize) -> Place {
        Place { line, character }
    }

    fn press(cx: &mut VisualTestContext, keys: &str) {
        cx.update(|window, cx| window.press(keys, cx));
        cx.run_until_parked();
    }

    fn secondary(key: &str) -> String {
        if cfg!(target_os = "macos") {
            format!("cmd-{key}")
        } else {
            format!("ctrl-{key}")
        }
    }

    #[test]
    fn ui_22_runs_split_at_line_breaks_and_place_the_caret_in_characters() {
        let runs = Runs::new("ä\nb", 0..0, 0);
        assert_eq!(runs.lines, ["ä\n", "b"]);
        // After the line break the caret starts the next line.
        assert_eq!(Runs::new("ä\nb", 3..3, 3).caret, at(1, 0));
        // Before it, it ends the first, one character in.
        assert_eq!(Runs::new("ä\nb", 2..2, 2).caret, at(0, 1));
        // A final line break and an empty field give the caret a line.
        assert_eq!(Runs::new("a\n", 2..2, 2).lines, ["a\n", ""]);
        assert_eq!(Runs::new("a\n", 2..2, 2).caret, at(1, 0));
        assert_eq!(Runs::new("", 0..0, 0).lines, [""]);
        // A selection made leftwards has its caret at its start.
        let left = Runs::new("abc", 1..3, 1);
        assert_eq!((left.anchor, left.caret), (at(0, 3), at(0, 1)));
        let nodes = Runs::new("é\n", 0..0, 0).nodes();
        assert_eq!(nodes[0].character_lengths(), [2, 1]);
        assert_eq!(nodes[0].value(), Some("é\n"));
    }

    #[gpui::test]
    fn ui_22_the_wrapper_follows_typing_deleting_selecting_and_pasting(cx: &mut TestAppContext) {
        let (view, cx) = probe(cx);
        let (subject, body) = cx.update(|_, cx| {
            let view = view.read(cx);
            (
                Field::Line(view.subject.clone()),
                Field::Lines(view.body.clone()),
            )
        });
        // The wrapper is the field a screen reader finds.
        let found = cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("subject")
        });
        assert_eq!(found.role(), Some(Role::TextInput));
        assert_eq!(found.label(), Some("Subject"));

        // Typing, and the caret moved back.
        cx.update(|window, cx| window.click("subject", cx));
        cx.update(|window, cx| window.input("ab", cx));
        press(cx, "left");
        press(cx, "left");
        let typed = runs(cx, &subject);
        assert_eq!(typed.lines, ["ab"]);
        assert_eq!((typed.anchor, typed.caret), (at(0, 0), at(0, 0)));
        assert_eq!(
            cx.update(|window, _| window.find("subject").value().map(str::to_owned)),
            Some("ab".to_owned())
        );

        // Lines, a deletion and a selection in the body.
        cx.update(|window, cx| window.click("body", cx));
        cx.update(|window, cx| window.input("one\ntwo!", cx));
        press(cx, "backspace");
        let deleted = runs(cx, &body);
        assert_eq!(deleted.lines, ["one\n", "two"]);
        assert_eq!(deleted.caret, at(1, 3));
        press(cx, "shift-left");
        press(cx, "shift-left");
        let selected = runs(cx, &body);
        assert_eq!((selected.anchor, selected.caret), (at(1, 3), at(1, 1)));

        // A paste replaces the selection.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("ins\nmore".into())));
        press(cx, &secondary("v"));
        let pasted = runs(cx, &body);
        assert_eq!(pasted.lines, ["one\n", "tins\n", "more"]);
        assert_eq!((pasted.anchor, pasted.caret), (at(2, 4), at(2, 4)));
    }

    /// ARCH-7: the runs of a 10,000-character body, after a keystroke,
    /// take under 2 ms. Timed, so it runs with `just timing`.
    #[gpui::test]
    #[ignore = "timed; run with `just timing`"]
    fn ui_22_the_runs_of_a_long_body_take_under_2_ms(cx: &mut TestAppContext) {
        let (view, cx) = probe(cx);
        let state = cx.update(|_, cx| view.read(cx).body.clone());
        let body = Field::Lines(state.clone());
        let long: String = (0..400)
            .map(|n| format!("line {n:>4} of the body here\n"))
            .collect();
        assert!(long.chars().count() >= 10_000);
        cx.update(|window, cx| state.update(cx, |state, cx| state.set_value(long, window, cx)));
        cx.update(|window, cx| window.click("body", cx));
        press(cx, "a");
        let mut slowest = Duration::ZERO;
        for _ in 0..20 {
            let started = Instant::now();
            let nodes = cx.update(|_, cx| Runs::of(&body, cx).nodes());
            slowest = slowest.max(started.elapsed());
            assert_eq!(nodes.len(), 401);
        }
        assert!(slowest < Duration::from_millis(2), "{slowest:?}");
    }
}
