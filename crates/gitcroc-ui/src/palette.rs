//! The command palette (UI-22), a port of rc.1's `ui/src/palette/Palette.tsx`: a
//! text box above the core's answer to it, virtualized (PERF-1). Each
//! change of the text asks the core again, off the UI thread (ARCH-7); the
//! list keeps the last answer until a newer one arrives, so typing never
//! waits, and an answer the core superseded, or one older than the one
//! shown, is dropped. The keys move the highlight and Enter chooses its
//! row: the window closes the palette and runs an operation as its place
//! in the window does, or selects a repo, group or branch in the tree. An
//! operation that cannot apply now says why and Enter does nothing.
//! Escape, the palette's key or a click outside it closes it.
//! The core finds, ranks and labels the rows (ARCH-6); names are text
//! only (SEC-3).

use gpui_component::scroll::ScrollableElement as _;
use std::rc::Rc;

use gitcroc_core::{PaletteItem, PaletteResult, PaletteSubject, WorkspaceId};
use gpui_component::FocusTrapElement;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, KeyBindingContextPredicate, ParentElement, Pixels,
    Render, Role, ScrollStrategy, SharedString, StatefulInteractiveElement, Styled, Subscription,
    TestSupportExt, UniformListScrollHandle, WeakEntity, Window, actions, div, px, uniform_list,
};

use crate::dialog::{self, Cancel, Closed, Modal};
use crate::keys;
use crate::outline::Outlined;
use crate::services::Services;
use crate::strings::{self, palette as text};
use crate::theme::ActiveTokens;
use crate::window::GitcrocWindow;

actions!(
    palette,
    [
        /// Highlights the next row.
        Down,
        /// Highlights the row above.
        Up,
        /// Highlights the row a page down.
        PageDown,
        /// Highlights the row a page up.
        PageUp,
        /// Highlights the first row.
        First,
        /// Highlights the last row.
        Last,
    ]
);

/// The key context of the palette.
const CONTEXT: &str = "GitcrocPalette";

/// The height of one row.
pub const ROW_HEIGHT: Pixels = px(28.);

/// The rows the list shows at once.
const VISIBLE_ROWS: usize = 12;

/// The palette's keys. They are bound on its text box, whose own keys for
/// the same keystrokes they replace.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(&format!("{CONTEXT} > Input"))
        .ok()
        .map(Rc::new);
    let bindings = [
        KeyBinding::load("down", Box::new(Down), context.clone(), false, None, cx),
        KeyBinding::load("up", Box::new(Up), context.clone(), false, None, cx),
        KeyBinding::load(
            "pagedown",
            Box::new(PageDown),
            context.clone(),
            false,
            None,
            cx,
        ),
        KeyBinding::load("pageup", Box::new(PageUp), context.clone(), false, None, cx),
        KeyBinding::load("home", Box::new(First), context.clone(), false, None, cx),
        KeyBinding::load("end", Box::new(Last), context.clone(), false, None, cx),
        KeyBinding::load("escape", Box::new(Cancel), context, false, None, cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// What an operation does when chosen: the window runs it after closing
/// the palette.
pub type Run = Box<dyn FnOnce(&mut GitcrocWindow, &mut Window, &mut Context<GitcrocWindow>)>;

/// What an operation does now, as its control in the window would: run, or
/// why it cannot apply.
pub enum Command {
    Run(Run),
    Cannot(SharedString),
}

impl Command {
    pub fn run(
        run: impl FnOnce(&mut GitcrocWindow, &mut Window, &mut Context<GitcrocWindow>) + 'static,
    ) -> Self {
        Self::Run(Box::new(run))
    }

    pub fn cannot(reason: impl Into<SharedString>) -> Self {
        Self::Cannot(reason.into())
    }

    /// Runs `run`, or says the control's reason, or the palette's own when
    /// the control gives none: a disabled control is `None`.
    pub fn of(run: Option<Run>, reason: Option<&str>) -> Self {
        match run {
            Some(run) => Self::Run(run),
            None => Self::cannot(reason.unwrap_or(text::CANNOT_APPLY).to_owned()),
        }
    }

    pub fn reason(&self) -> Option<&SharedString> {
        match self {
            Self::Run(_) => None,
            Self::Cannot(reason) => Some(reason),
        }
    }
}

/// What to select for an operation on `subject` the selection does not
/// give.
pub fn missing_subject(subject: PaletteSubject) -> &'static str {
    match subject {
        PaletteSubject::TargetSet => text::SELECT_TARGET_SET,
        PaletteSubject::Branch => text::SELECT_BRANCH,
        PaletteSubject::Tag => text::SELECT_TAG,
        PaletteSubject::Stash => text::SELECT_STASH,
        PaletteSubject::Remote => text::SELECT_REMOTE,
        PaletteSubject::Commit => text::SELECT_COMMIT,
        PaletteSubject::File => text::SELECT_FILE,
        PaletteSubject::Conflict => text::SELECT_CONFLICT,
        PaletteSubject::App => text::SELECT_APP,
    }
}

/// A row was chosen: an operation that can run now, or a repo, group or
/// branch to select.
#[derive(Debug, Clone)]
pub struct Chosen(pub PaletteItem);

/// The command palette of one workspace.
pub struct Palette {
    services: Services,
    workspace: WorkspaceId,
    /// The window whose places say what each operation does now.
    window: WeakEntity<GitcrocWindow>,
    input: Entity<InputState>,
    result: Option<PaletteResult>,
    error: Option<SharedString>,
    highlight: usize,
    /// The newest request sent, and the newest one whose answer is shown.
    sent: u64,
    shown: u64,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _input_events: Subscription,
}

impl EventEmitter<Closed> for Palette {}
impl EventEmitter<Chosen> for Palette {}

impl Palette {
    pub fn new(
        services: Services,
        workspace: WorkspaceId,
        window_view: WeakEntity<GitcrocWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(text::SEARCH));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
            InputEvent::Change => this.query(cx),
            InputEvent::PressEnter { .. } => this.choose(this.highlight, cx),
            InputEvent::Focus | InputEvent::Blur => {}
        });
        let mut this = Self {
            services,
            workspace,
            window: window_view,
            input,
            result: None,
            error: None,
            highlight: 0,
            sent: 0,
            shown: 0,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            _input_events: input_events,
        };
        this.query(cx);
        this
    }

    /// The answer shown.
    pub fn result(&self) -> Option<&PaletteResult> {
        self.result.as_ref()
    }

    /// The highlighted row.
    pub fn highlight(&self) -> usize {
        self.highlight
    }

    /// The number of the newest request sent.
    pub fn sent(&self) -> u64 {
        self.sent
    }

    fn items(&self) -> &[PaletteItem] {
        self.result.as_ref().map_or(&[][..], |result| &result.items)
    }

    /// Asks the core for the text's rows.
    fn query(&mut self, cx: &mut Context<Self>) {
        self.sent += 1;
        let request = self.sent;
        let (workspace, typed) = (
            self.workspace.clone(),
            self.input.read(cx).value().to_string(),
        );
        let call = self
            .services
            .with_app(move |app| async move { app.palette_query(workspace, typed).await });
        cx.spawn(async move |this, cx| {
            let answer = call.await;
            let _ = this.update(cx, |this, cx| {
                let answer = match answer {
                    Some(answer) => answer.map_err(|error| error.to_string()),
                    None => Err(crate::strings::operations::STOPPED.to_owned()),
                };
                this.answer(request, answer, cx);
            });
        })
        .detach();
    }

    /// The core's answer to request `request`: `None` when a newer query
    /// superseded it. An older answer never replaces a newer one, and only
    /// the newest request's failure is shown.
    pub fn answer(
        &mut self,
        request: u64,
        answer: Result<Option<PaletteResult>, String>,
        cx: &mut Context<Self>,
    ) {
        match answer {
            Ok(Some(result)) if request > self.shown => {
                self.shown = request;
                self.result = Some(result);
                self.error = None;
                self.highlight = 0;
                self.scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            Err(error) if request == self.sent => {
                self.error = Some(text::cannot_read(&error).into());
            }
            // The core numbers queries as they reach it, so one sent just
            // before may have overtaken the newest: ask again for the text.
            Ok(None) if request == self.sent => {
                self.query(cx);
                return;
            }
            _ => return,
        }
        cx.notify();
    }

    /// What operation row `item` does now; `None` for the other rows,
    /// which always run.
    fn command_of(&self, item: &PaletteItem, cx: &App) -> Option<Command> {
        let PaletteItem::Operation { op, subject, .. } = item else {
            return None;
        };
        let command = self
            .window
            .upgrade()
            .and_then(|window| window.read(cx).palette_command(&self.workspace, *op, cx));
        Some(command.unwrap_or_else(|| Command::cannot(missing_subject(*subject))))
    }

    /// Chooses row `index`, unless it is an operation that cannot apply.
    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(item) = self.items().get(index).cloned() else {
            return;
        };
        if self
            .command_of(&item, cx)
            .is_some_and(|command| command.reason().is_some())
        {
            return;
        }
        cx.emit(Chosen(item));
    }

    fn move_to(&mut self, index: isize, cx: &mut Context<Self>) {
        let Some(last) = self.items().len().checked_sub(1) else {
            return;
        };
        let next = usize::try_from(index.max(0)).unwrap_or_default().min(last);
        self.highlight = next;
        self.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let at = isize::try_from(self.highlight).unwrap_or(isize::MAX);
        self.move_to(at.saturating_add(step), cx);
    }

    /// A page is the rows on screen but one.
    fn page() -> isize {
        isize::try_from(VISIBLE_ROWS - 1).unwrap_or(1)
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Self::page(), cx);
    }

    fn page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-Self::page(), cx);
    }

    fn first(&mut self, _: &First, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn last(&mut self, _: &Last, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(isize::MAX, cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Closed);
    }

    fn rows(
        &mut self,
        range: std::ops::Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .filter_map(|index| {
                let item = self.items().get(index)?.clone();
                Some(self.row(index, &item, cx))
            })
            .collect()
    }

    fn row(&self, index: usize, item: &PaletteItem, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *cx.tokens();
        let (kind, name, detail): (&str, &str, Option<String>) = match item {
            PaletteItem::Operation { label, .. } => (text::OPERATION, label, None),
            PaletteItem::Repo { name, branch, .. } => (text::REPO, name, branch.clone()),
            PaletteItem::Group { name, members, .. } => {
                (text::GROUP, name, Some(text::members(*members)))
            }
            PaletteItem::Branch {
                name,
                repo_name,
                remote,
                ..
            } => (
                if remote.is_some() {
                    text::REMOTE_BRANCH
                } else {
                    text::BRANCH
                },
                name,
                Some(text::in_repo(repo_name)),
            ),
        };
        let reason = self
            .command_of(item, cx)
            .and_then(|command| command.reason().cloned());
        let mac = cfg!(target_os = "macos");
        let shortcut = match item {
            PaletteItem::Operation { op, .. } => {
                keys::palette_shortcut(*op).map(|keys| SharedString::from(keys.label(mac)))
            }
            _ => None,
        };
        let highlighted = index == self.highlight;
        let part = |id: &str, said: SharedString| {
            div()
                .id(ElementId::Name(format!("palette-row-{index}-{id}").into()))
                .test_support()
                .role(Role::Label)
                .aria_label(said.clone())
                .flex_shrink_0()
                .child(said)
        };
        let name: SharedString = name.to_owned().into();
        div()
            .id(ElementId::NamedInteger("palette-row".into(), index as u64))
            .test_support()
            .role(Role::ListBoxOption)
            .aria_label(name.clone())
            .aria_selected(highlighted)
            .when(highlighted, |row| row.aria_active_descendant())
            // Why it cannot run, then its place in the results (M6-25a).
            .aria_description(strings::a11y::describe(&[
                reason.as_deref().unwrap_or_default(),
                &strings::a11y::position(index, self.items().len()),
            ]))
            .when_some(shortcut.clone(), |row, keys| row.aria_keyshortcuts(keys))
            .h(ROW_HEIGHT)
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .relative()
            .when(highlighted, |row| row.bg(tokens.bg_3))
            .selection_mark(highlighted, &tokens)
            .when(reason.is_some(), |row| {
                row.text_color(tokens.text_secondary)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.highlight = index;
                this.choose(index, cx);
            }))
            .child(part("kind", kind.into()).text_color(tokens.text_secondary))
            .child(
                div()
                    .id(ElementId::Name(format!("palette-row-{index}-text").into()))
                    .test_support()
                    .role(Role::Label)
                    .aria_label(name.clone())
                    .min_w_0()
                    .truncate()
                    .child(name),
            )
            .children(detail.map(|detail| {
                part("detail", detail.into())
                    .flex_shrink(1.)
                    .min_w_0()
                    .truncate()
                    .text_color(tokens.text_secondary)
            }))
            .child(div().flex_1())
            .children(reason.map(|reason| {
                part("reason", reason)
                    .flex_shrink(1.)
                    .min_w_0()
                    .truncate()
                    .text_sm()
            }))
            .children(shortcut.map(|keys| {
                part("shortcut", keys)
                    .px_1()
                    .text_sm()
                    .border_1()
                    .border_color(tokens.border)
                    .rounded(px(3.))
            }))
            .into_any_element()
    }

    /// The line under the list: more matched than are shown, or nothing.
    fn more(&self) -> Option<SharedString> {
        let result = self.result.as_ref()?;
        let shown = result.items.len();
        if u64::from(result.total) > shown as u64 {
            Some(text::more(shown, result.total).into())
        } else if result.total == 0 {
            Some(text::NOTHING.into())
        } else {
            None
        }
    }
}

impl Modal for Palette {
    /// The text box has the focus.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// A click beside it closes it (UI-22).
    fn closes_on_backdrop(&self) -> bool {
        true
    }
}

impl Focusable for Palette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let count = self.items().len();
        let error = self.error.clone().map(|error| {
            div()
                .id("palette-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .text_color(tokens.danger)
                .child(error)
        });
        let more = self.more().map(|said| {
            div()
                .id("palette-more")
                .test_support()
                .role(Role::Status)
                .aria_label(said.clone())
                .text_sm()
                .text_color(tokens.text_secondary)
                .child(said)
        });
        let title: SharedString = text::LABEL.into();
        dialog::frame("palette-frame", Role::Dialog, &title, cx)
            .test_support()
            .w(px(600.))
            .track_focus(&self.focus)
            .key_context(format!("{} {CONTEXT}", dialog::CONTEXT).as_str())
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::first))
            .on_action(cx.listener(Self::last))
            // A click inside does not reach the backdrop.
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation()
            })
            .child(
                Input::new(&self.input)
                    .id("palette-input")
                    .aria_label(text::LABEL),
            )
            .children(error)
            .child(
                div()
                    .id("palette-list")
                    .relative()
                    .test_support()
                    .role(Role::ListBox)
                    .aria_label(text::RESULTS)
                    .h(ROW_HEIGHT * VISIBLE_ROWS as f32)
                    .w_full()
                    .child(
                        uniform_list("palette-rows", count, cx.processor(Self::rows))
                            .track_scroll(&self.scroll)
                            .size_full(),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
            .children(more)
            // The trap takes the place of the frame, under the palette's id.
            .focus_trap("palette", &self.focus)
    }
}
