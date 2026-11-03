//! The Settings dialog, a port of the frame of `ui/src/SettingsDialog.tsx`:
//! a tab strip, one tab's settings shown at a time, and which gitcroc is
//! running below them. It has only the AI tab until M6-26 adds Appearance,
//! Git and Updates. The arrow keys, Home and End move between the tabs,
//! and the focus starts on the tab's first control (UI-22). A window has
//! one; opening it again focuses it.

use std::cell::Cell;
use std::rc::Rc;

use gpui_component::FocusTrapElement;
use gpui_component::button::Button;
use gpui_component::input::InputState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, KeyBindingContextPredicate, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, Window, actions, div, px,
};

use crate::ai::Ai;
use crate::ai_section::AiSection;
use crate::dialog::{self, Cancel, Closed, Modal};
use crate::services::Services;
use crate::strings::{common, settings as text};
use crate::theme::ActiveTokens;

actions!(
    settings_dialog,
    [
        /// Shows the tab left of the focused one.
        PreviousTab,
        /// Shows the tab right of the focused one.
        NextTab,
        FirstTab,
        LastTab,
    ]
);

/// The key context of the tab strip.
const TABS_CONTEXT: &str = "GitcrocSettingsTabs";

/// The keys of the tab strip.
pub fn bind(cx: &mut App) {
    let context = KeyBindingContextPredicate::parse(TABS_CONTEXT)
        .ok()
        .map(Rc::new);
    let bind = |keys: &str, action: Box<dyn gpui_kit::Action>, cx: &mut App| {
        KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
    };
    let bindings = [
        bind("left", Box::new(PreviousTab), cx),
        bind("right", Box::new(NextTab), cx),
        bind("home", Box::new(FirstTab), cx),
        bind("end", Box::new(LastTab), cx),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// A tab of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Ai,
}

impl SettingsTab {
    /// Every tab, in order.
    const ALL: [Self; 1] = [Self::Ai];

    fn id(self) -> &'static str {
        match self {
            Self::Ai => "settings-tab-ai",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ai => text::TAB_AI,
        }
    }
}

pub struct SettingsDialog {
    tab: SettingsTab,
    ai: Entity<AiSection>,
    about: SharedString,
    focus: FocusHandle,
    /// Each tab's focus, in [`SettingsTab::ALL`]'s order.
    tab_focus: Vec<FocusHandle>,
    /// The focus moves from the tab to its first control once drawn.
    to_first_control: Cell<bool>,
}

impl EventEmitter<Closed> for SettingsDialog {}

impl SettingsDialog {
    pub fn new(
        services: &Services,
        ai: Entity<Ai>,
        at: SettingsTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let info = services.app().app_info();
        let section = cx.new(|cx| AiSection::new(ai, window, cx));
        Self {
            tab: at,
            ai: section,
            about: text::about(&info.name, &info.version, &info.os, &info.arch).into(),
            focus: cx.focus_handle(),
            tab_focus: SettingsTab::ALL
                .iter()
                .map(|_| cx.focus_handle().tab_stop(true))
                .collect(),
            to_first_control: Cell::new(false),
        }
    }

    /// Shows `tab`, with the focus on its first control.
    pub fn show(&mut self, tab: SettingsTab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.focus_first(window, cx);
        cx.notify();
    }

    pub fn tab(&self) -> SettingsTab {
        self.tab
    }

    /// The AI tab's key field, while one is shown (SEC-1).
    pub fn key_field(&self, cx: &App) -> Option<Entity<InputState>> {
        self.ai.read(cx).key_field(cx)
    }

    fn at(&self) -> usize {
        SettingsTab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or_default()
    }

    /// Shows the tab at `to` and focuses it.
    fn move_to(&mut self, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(tab), Some(focus)) = (SettingsTab::ALL.get(to), self.tab_focus.get(to)) {
            self.tab = *tab;
            window.focus(focus, cx);
            cx.notify();
        }
    }

    fn previous(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        let count = SettingsTab::ALL.len();
        self.move_to((self.at() + count - 1) % count, window, cx);
    }

    fn next(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to((self.at() + 1) % SettingsTab::ALL.len(), window, cx);
    }

    fn first(&mut self, _: &FirstTab, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
    }

    fn last(&mut self, _: &LastTab, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(SettingsTab::ALL.len() - 1, window, cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai
            .update(cx, |section, cx| section.clear_key(window, cx));
        cx.emit(Closed);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}

impl Modal for SettingsDialog {
    /// The shown tab has the focus, then the first control of its
    /// settings, once they are drawn.
    fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if let Some(focus) = self.tab_focus.get(self.at()) {
            window.focus(focus, cx);
        }
        self.to_first_control.set(true);
    }
}

impl Focusable for SettingsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.to_first_control.take() {
            // The tab stops are known once this frame is drawn.
            cx.defer_in(window, |_, window, cx| window.focus_next(cx));
        }
        let title = SharedString::from(text::TITLE);
        let tokens = *cx.tokens();
        let tabs = SettingsTab::ALL.iter().enumerate().map(|(at, tab)| {
            let tab = *tab;
            let selected = self.tab == tab;
            div()
                .id(tab.id())
                .test_support()
                .role(Role::Tab)
                .aria_label(tab.label())
                .aria_selected(selected)
                .track_focus(&self.tab_focus[at])
                .px_3()
                .py_1()
                .border_b_2()
                .border_color(if selected {
                    tokens.selection_mark
                } else {
                    tokens.border
                })
                .when(!selected, |this| this.text_color(tokens.text_secondary))
                .on_click(cx.listener(move |this, _, window, cx| this.move_to(at, window, cx)))
                .child(tab.label())
        });
        let panel = match self.tab {
            SettingsTab::Ai => self.ai.clone(),
        };
        dialog::frame("settings-frame", Role::Dialog, &title, cx)
            .test_support()
            .track_focus(&self.focus)
            .key_context(dialog::CONTEXT)
            .on_action(cx.listener(Self::cancel))
            .w(px(600.))
            .child(
                div()
                    .id("settings-tabs")
                    .test_support()
                    .role(Role::TabList)
                    .aria_label(text::TABS)
                    .key_context(TABS_CONTEXT)
                    .on_action(cx.listener(Self::previous))
                    .on_action(cx.listener(Self::next))
                    .on_action(cx.listener(Self::first))
                    .on_action(cx.listener(Self::last))
                    .flex()
                    .flex_row()
                    .gap_1()
                    .children(tabs),
            )
            .child(
                div()
                    .id("settings-panel")
                    .test_support()
                    .role(Role::TabPanel)
                    .aria_label(self.tab.label())
                    .max_h(px(520.))
                    .overflow_y_scroll()
                    .child(panel),
            )
            .child(
                div()
                    .id("settings-about")
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(self.about.clone())
                    .text_sm()
                    .child(self.about.clone()),
            )
            .child(
                dialog::actions_row().child(
                    Button::new("settings-close")
                        .label(common::CLOSE)
                        .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                ),
            )
            // The trap takes the place of the frame, under the dialog's id.
            .focus_trap("settings-dialog", &self.focus)
    }
}
