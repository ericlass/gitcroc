//! Retained, single-buffer conflict editing. No services or repo writes (OPS-9).
mod buffer;
mod card;
mod look;
mod overlay;
pub mod recovery;
mod retained;
mod syntax;
use crate::keys::{NextHunk, PreviousHunk};
use crate::strings::conflict as text;
use crate::theme::code_themes::Syntax;
use crate::theme::{ActiveTokens, AppTheme};
pub use buffer::{Buffer, Hunk, Take};
use gitcroc_core::{ConflictChunk, ConflictLabels, ProposedHunk};
use gpui_component::input::{Editor, EditorState, InputEvent};
use gpui_kit::base::input::{RangeDecoration, RangeDecorationCollection, RopeExt};
use gpui_kit::{prelude::*, *};
pub use look::Look;
use overlay::Measured;
use recovery::{Recovery, Snapshot};
pub use retained::{EditorKey, Editors};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

actions!(
    conflict,
    [
        NextConflict,
        PreviousConflict,
        TakeOurs,
        TakeTheirs,
        TakeBoth,
        /// F7: the next hunk's first control (UI-22).
        NextControl,
        PreviousControl,
        /// Escape on a hunk's control: back to the text at that hunk.
        ReturnToText,
        /// Alt+4, Alt+5 and Alt+6: the proposal at the cursor (AI-11).
        AcceptProposal,
        RejectProposal,
        EditProposal
    ]
);
pub fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("alt-down", NextConflict, Some("GitcrocConflict > Input")),
        KeyBinding::new("alt-up", PreviousConflict, Some("GitcrocConflict > Input")),
        KeyBinding::new("f7", NextControl, Some("GitcrocConflict")),
        KeyBinding::new("shift-f7", PreviousControl, Some("GitcrocConflict")),
        KeyBinding::new("escape", ReturnToText, Some(overlay::CONTROLS)),
        KeyBinding::new("alt-4", AcceptProposal, Some("GitcrocConflict > Input")),
        KeyBinding::new("alt-5", RejectProposal, Some("GitcrocConflict > Input")),
        KeyBinding::new("alt-6", EditProposal, Some("GitcrocConflict > Input")),
        KeyBinding::new("alt-1", TakeOurs, Some("GitcrocConflict > Input")),
        KeyBinding::new("alt-2", TakeTheirs, Some("GitcrocConflict > Input")),
        KeyBinding::new("alt-3", TakeBoth, Some("GitcrocConflict > Input")),
    ]);
    // The proposal's editor: Ctrl or Cmd and Enter accepts, Escape cancels.
    // `secondary` needs the platform, which `KeyBinding::new` cannot see.
    let accept = if cx.platform_info().is_mac() {
        "cmd-enter"
    } else {
        "ctrl-enter"
    };
    let context = KeyBindingContextPredicate::parse(&format!("{} > Input", card::EDITING))
        .ok()
        .map(Rc::new);
    let load = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        load(accept, Box::new(card::AcceptEdited), cx),
        load("escape", Box::new(card::CancelEdit), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}
struct Tracked {
    hunk: Hunk,
    anchor: RangeDecorationCollection,
    // The spike's real-text ghost, with recovery snapshots at document edges.
    before: RangeDecorationCollection,
    taken: bool,
    untouched: bool,
}
impl Tracked {
    fn range(&self, cx: &App) -> Range<usize> {
        self.anchor
            .get_ranges(cx)
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                let at = self.before.get_ranges(cx).first().map_or(0, |r| r.end);
                at..at
            })
    }
}
fn decoration(range: Range<usize>) -> RangeDecoration {
    RangeDecoration::new(range).with_color(Hsla::transparent_black())
}
fn anchor(
    state: &mut EditorState,
    range: Range<usize>,
    cx: &mut Context<EditorState>,
) -> RangeDecorationCollection {
    state.create_range_decorations_collection(vec![decoration(range)], cx)
}
pub struct ConflictEditor {
    pub editor: Entity<EditorState>,
    hunks: Vec<Tracked>,
    unsaved: bool,
    generation: u64,
    recovery: Option<Task<Recovery>>,
    pending_takes: Vec<usize>,
    edits: u64,
    prepared: Option<Buffer>,
    _changes: Subscription,
    _observe: Subscription,
    restore_focus: bool,
    labels: ConflictLabels,
    path: Option<String>,
    lines: syntax::Lines,
    show_base: HashSet<usize>,
    /// The supplied proposal's hunks, and what was done with each (AI-11).
    proposal: Vec<ProposedHunk>,
    reviews: HashMap<usize, card::Review>,
    /// The first control of the hunk F7 went to.
    controls: FocusHandle,
    control_hunk: Option<usize>,
    laid_out: usize,
    /// The editor's layout as last painted, which the overlays follow.
    measured: Rc<Cell<Option<Measured>>>,
}
impl ConflictEditor {
    pub fn new(buffer: Buffer, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let labels = buffer.labels;
        let initial: SharedString = buffer.text.into();
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language("text")
                .line_number(true)
                .soft_wrap(false)
                .folding(false)
                .scroll_beyond_last_line(None);
            state.set_value(initial.clone(), window, cx);
            state
        });
        let hunks: Vec<Tracked> = editor.update(cx, |state, cx| {
            buffer
                .hunks
                .into_iter()
                .map(|hunk| Tracked {
                    anchor: anchor(state, hunk.range.clone(), cx),
                    before: anchor(state, 0..hunk.range.start, cx),
                    hunk,
                    taken: false,
                    untouched: true,
                })
                .collect()
        });
        let snapshot = Snapshot {
            pinned: false,
            untouched: vec![true; hunks.len()],
            unsaved: false,
            text: editor.read(cx).text().clone(),
            hunks: hunks
                .iter()
                .map(|h| (h.hunk.range.clone(), false))
                .collect(),
        };
        let recovery = Some(cx.background_spawn(async move { Recovery::new(snapshot) }));
        // Scrolling and caret moves redraw the overlays in the same frame.
        let observe = cx.observe(&editor, |_, _, cx| cx.notify());
        let changes = cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.generation += 1;
                this.prepared = None;
                this.refresh(cx);
            }
        });
        Self {
            editor,
            hunks,
            unsaved: false,
            generation: 0,
            recovery,
            pending_takes: Vec::new(),
            edits: 0,
            prepared: None,
            _changes: changes,
            _observe: observe,
            restore_focus: false,
            labels,
            path: None,
            lines: syntax::Lines::default(),
            show_base: HashSet::new(),
            proposal: Vec::new(),
            reviews: HashMap::new(),
            controls: cx.focus_handle(),
            control_hunk: None,
            laid_out: 0,
            measured: Rc::default(),
        }
    }
    /// Highlights the buffer as the file at `path`; a type syntect does not
    /// know stays plain text (UI-24).
    pub fn set_language(&mut self, path: &str, cx: &mut Context<Self>) {
        self.path = Some(path.to_owned());
        self.lines = syntax::Lines::default();
        let factory = syntax::factory(path, self.lines.clone());
        self.editor
            .update(cx, |state, cx| state.set_highlighter_factory(factory, cx));
        cx.notify();
    }
    /// The syntax at `offset` as last highlighted; `None` for plain text.
    pub fn syntax_at(&self, offset: usize, _cx: &App) -> Option<Syntax> {
        syntax::at(&self.lines, offset)
    }
    /// The colours in use: the code theme's, whatever the app theme.
    pub fn look(&self, cx: &App) -> Look {
        Look::of(&AppTheme::global(cx).code_theme().colors)
    }
    /// Where an untouched hunk's three marker lines start.
    pub fn marker_offsets(&self, id: usize, cx: &App) -> Option<(usize, usize, usize)> {
        let (_, range, _) = self
            .ranges(cx)
            .into_iter()
            .find(|(at, _, untouched)| *at == id && *untouched)?;
        let hunk = &self.hunks[id].hunk;
        Some((
            range.start,
            range.start + hunk.separator,
            range.start + hunk.end,
        ))
    }
    /// How many hunks the last frame laid out overlays for (PERF-1).
    pub fn laid_out(&self) -> usize {
        self.laid_out
    }
    /// F7 and Shift+F7 (UI-22): the next or previous hunk, and its Take
    /// ours, which Tab goes on from and Escape leaves for the text.
    fn focus_control(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(forward, window, cx);
        let cursor = self.editor.read(cx).cursor();
        self.control_hunk = self
            .ranges(cx)
            .into_iter()
            .find(|(_, range, untouched)| *untouched && range.contains(&cursor))
            .map(|(id, _, _)| id);
        if self.control_hunk.is_some() {
            window.focus(&self.controls, cx);
        }
        cx.notify();
    }
    fn return_to_text(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let at = self
            .marker_offsets(id, cx)
            .map(|(head, _, _)| head + self.hunks[id].hunk.head);
        self.editor.update(cx, |state, cx| {
            if let Some(at) = at {
                state.set_selected_range(at..at, cx);
            }
            state.focus(window, cx);
        });
    }
    pub fn value(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }
    pub fn unsaved(&self, _cx: &App) -> bool {
        self.unsaved
    }
    /// Live regions include manually edited hunks; only untouched ones have actions.
    pub fn ranges(&self, cx: &App) -> Vec<(usize, Range<usize>, bool)> {
        self.hunks
            .iter()
            .filter(|h| !h.taken)
            .map(|h| {
                let range = h.range(cx);
                (h.hunk.id, range, h.untouched)
            })
            .collect()
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.edits += 1;
        let edits = self.edits;
        let snapshot = Snapshot {
            pinned: false,
            untouched: Vec::new(),
            unsaved: true,
            text: self.editor.read(cx).text().clone(),
            hunks: self
                .hunks
                .iter()
                .map(|h| {
                    let range = h.range(cx);
                    (range, h.taken)
                })
                .collect(),
        };
        self.unsaved = true;
        for h in &mut self.hunks {
            h.untouched = false;
        }
        let takes = std::mem::take(&mut self.pending_takes);
        let previous = self.recovery.take();
        // Serialize recovery even when several edits arrive before a frame.
        self.recovery = Some(cx.spawn(async move |this, cx| {
            let mut recovery = match previous {
                Some(task) => task.await,
                None => Recovery::new(snapshot.clone()),
            };
            let (recovery, snapshot) = cx
                .background_spawn(async move {
                    let snapshot = recovery.record(snapshot, &takes);
                    (recovery, snapshot)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.edits != edits {
                    return;
                }
                this.unsaved = snapshot.unsaved;
                for ((h, (range, taken)), untouched) in this
                    .hunks
                    .iter_mut()
                    .zip(snapshot.hunks)
                    .zip(snapshot.untouched)
                {
                    h.taken = taken;
                    h.untouched = untouched;
                    h.anchor.set(vec![decoration(range.clone())], cx);
                    h.before.set(vec![decoration(0..range.start)], cx);
                }
                cx.notify();
            });
            recovery
        }));
        cx.notify();
    }
    pub fn take(
        &mut self,
        id: usize,
        side: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(h) = self.hunks.get(id) else {
            return false;
        };
        self.accept(id, &h.hunk.taken(side), window, cx)
    }
    /// Supplied proposal text follows exactly the same atomic path as Take (AI-12).
    pub fn accept(
        &mut self,
        id: usize,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((_, range, true)) = self.ranges(cx).into_iter().find(|(at, _, _)| *at == id)
        else {
            return false;
        };
        self.hunks[id].taken = true;
        self.pending_takes.push(id);
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(range.clone(), cx);
            state.replace(buffer::normalize(text), window, cx);
            state.set_selected_range(range.start..range.start, cx);
            state.focus(window, cx);
        });
        true
    }
    pub fn navigate(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.editor.read(cx).cursor();
        let ranges = self.ranges(cx);
        let current = ranges.iter().find(|(_, r, _)| r.contains(&cursor));
        let limit = current.map_or(cursor + 1, |(_, r, _)| r.start);
        let target = if forward {
            ranges
                .iter()
                .find(|(_, r, _)| r.start > cursor)
                .or(ranges.first())
        } else {
            ranges
                .iter()
                .rev()
                .find(|(_, r, _)| r.start < limit)
                .or(ranges.last())
        };
        if let Some((id, range, untouched)) = target {
            let offset = range.start
                + if *untouched {
                    self.hunks[*id].hunk.head
                } else {
                    0
                };
            self.editor.update(cx, |state, cx| {
                state.set_selected_range(offset..offset, cx);
                state.focus(window, cx);
                let line = state.text().offset_to_point(offset).row;
                let y = line as f32 * state.line_height().unwrap_or(px(20.))
                    - state.input_bounds().size.height / 2.;
                state.set_scroll_offset(point(px(0.), -y.max(px(0.))), cx);
            });
        }
    }
    /// The live hunk the caret is in.
    fn hunk_at_cursor(&self, cx: &App) -> Option<usize> {
        let cursor = self.editor.read(cx).cursor();
        self.ranges(cx)
            .into_iter()
            .find(|(_, r, _)| r.contains(&cursor))
            .map(|(id, _, _)| id)
    }
    fn take_at_cursor(&mut self, side: Take, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.editor.read(cx).cursor();
        if let Some((id, _, _)) = self
            .ranges(cx)
            .into_iter()
            .find(|(_, r, _)| r.contains(&cursor))
        {
            self.take(id, side, window, cx);
        }
    }
    /// Call before hiding; returning only restores focus if no other control has it.
    pub fn hide(&mut self, window: &Window, cx: &App) {
        self.restore_focus = self.editor.read(cx).focus_handle(cx).is_focused(window);
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restore_focus && window.focused(cx).is_none() {
            self.editor.update(cx, |state, cx| state.focus(window, cx));
        }
        self.restore_focus = false;
    }
    /// Builds chunks off the UI thread. A later request invalidates the earlier one.
    pub fn prepare(
        &mut self,
        chunks: Vec<ConflictChunk>,
        labels: ConflictLabels,
        cx: &mut Context<Self>,
    ) {
        self.generation += 1;
        let generation = self.generation;
        self.prepared = None;
        cx.spawn(async move |this, cx| {
            let buffer = cx
                .background_spawn(async move { Buffer::build(&chunks, &labels) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.prepared = Some(buffer);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub fn prepared(&self) -> Option<&Buffer> {
        self.prepared.as_ref()
    }
    pub fn install_prepared(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(buffer) = self.prepared.take() {
            let generation = self.generation + 1;
            let path = self.path.take();
            *self = Self::new(buffer, window, cx);
            self.generation = generation;
            if let Some(path) = path {
                self.set_language(&path, cx);
            }
        }
    }
}
/// A layer over the editor's viewport that clips what is outside it.
fn clip(viewport: Option<Bounds<Pixels>>) -> Div {
    let viewport = viewport.unwrap_or_default();
    div()
        .absolute()
        .left(viewport.origin.x)
        .top(viewport.origin.y)
        .w(viewport.size.width)
        .h(viewport.size.height)
        .overflow_hidden()
}
impl Render for ConflictEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.look(cx);
        let tokens = *cx.tokens();
        let overlays = self.overlays(&look, &tokens, cx);
        let (editor, style) = (self.editor.clone(), look.editor_style());
        let (checked, measured, this) =
            (self.editor.clone(), self.measured.clone(), cx.weak_entity());
        let scroll = self.editor.read(cx).scroll_offset();
        let viewport = self.measured.get().map(|m| m.viewport);
        div()
            .key_context("GitcrocConflict")
            .tab_group()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(look.background)
            .text_color(look.text)
            .on_action(
                cx.listener(|this, _: &NextConflict, window, cx| this.navigate(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &PreviousConflict, window, cx| {
                this.navigate(false, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NextHunk, window, cx| this.focus_control(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &PreviousHunk, window, cx| {
                this.focus_control(false, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NextControl, window, cx| {
                    this.focus_control(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &PreviousControl, window, cx| {
                this.focus_control(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TakeOurs, window, cx| {
                this.take_at_cursor(Take::Ours, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TakeTheirs, window, cx| {
                this.take_at_cursor(Take::Theirs, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TakeBoth, window, cx| {
                this.take_at_cursor(Take::Both, window, cx)
            }))
            .on_action(cx.listener(|this, _: &AcceptProposal, window, cx| {
                if let Some(hunk) = this.hunk_at_cursor(cx) {
                    this.accept_proposal(hunk, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &RejectProposal, window, cx| {
                if let Some(hunk) = this.hunk_at_cursor(cx) {
                    this.reject_proposal(hunk, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &EditProposal, window, cx| {
                if let Some(hunk) = this.hunk_at_cursor(cx) {
                    this.edit_proposal(hunk, window, cx);
                }
            }))
            .child(clip(viewport).children(overlays.rows))
            // gpui-component's editor takes the app theme's colours each
            // frame; the code theme's go over them before it prepaints.
            .child(
                canvas(
                    move |_, _, cx| editor.update(cx, |state, _| state.set_editor_style(style)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_0(),
            )
            .child(
                Editor::new(&self.editor)
                    .h_full()
                    .bordered(false)
                    .bg(Hsla::transparent_black())
                    .text_color(look.text)
                    .when_some(self.path.as_deref(), |editor, path| {
                        editor.aria_label(text::resolution(path))
                    }),
            )
            .child(clip(viewport).children(overlays.headers))
            .child(clip(viewport).children(overlays.cards))
            // Measures the editor once it painted. A layout that moved after
            // the overlays were placed (a deferred scroll, a new font size)
            // places them again on the next frame.
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, _, cx| {
                        let now = Measured::read(checked.read(cx), bounds);
                        let before = measured.replace(now);
                        if now.is_some_and(|now| now.moved(before, scroll)) {
                            cx.defer(move |cx| {
                                let _ = this.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
}
