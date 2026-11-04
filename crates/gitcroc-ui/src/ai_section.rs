//! The AI tab of Settings, a port of `ui/src/ai/AiSection.tsx`. First the
//! AI features switch (AI-16): off, it is all the tab shows. On, the
//! providers with the default marked (AI-1), Edit, Remove and Set as
//! default, the CLI the core found and offers (AI-2), Add provider, the
//! exclude list (AI-4) and AI debug logging (AI-7). Everything comes from
//! the core's view (ARCH-6). With read-only settings (DATA-1) every
//! control is disabled.

use gitcroc_app::ai::{DetectedCli, ProviderView};
use gitcroc_core::ProviderId;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, Role, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    TestSupportExt, Window, div, px,
};

use crate::ai::Ai;
use crate::provider_form::{FormDone, ProviderForm, self_test_text};
use crate::settings::SettingsModel;
use crate::strings::{ai as text, common};
use crate::theme::ActiveTokens;

struct OpenForm {
    form: Entity<ProviderForm>,
    _done: Subscription,
}

pub struct AiSection {
    ai: Entity<Ai>,
    /// The settings, whose read-only state disables the tab (DATA-1).
    settings: Entity<SettingsModel>,
    /// The form shown in place of the list.
    form: Option<OpenForm>,
    /// The provider whose removal is asked about.
    removing: Option<ProviderId>,
    error: Option<SharedString>,
    exclude: Entity<TextareaState>,
    /// The exclude list was edited and not saved yet.
    exclude_edited: bool,
    _subscriptions: Vec<Subscription>,
}

impl AiSection {
    pub fn new(
        ai: Entity<Ai>,
        settings: Entity<SettingsModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let exclude = cx.new(|cx| TextareaState::new(window, cx).rows(4));
        let subscriptions = vec![
            cx.observe_in(&ai, window, |this, _, window, cx| {
                this.sync_exclude(window, cx);
                cx.notify();
            }),
            cx.subscribe_in(&exclude, window, |this, _, event: &InputEvent, _, cx| {
                match event {
                    InputEvent::Change => this.exclude_edited = true,
                    // Leaving the field saves it.
                    InputEvent::Blur => this.save_exclude(cx),
                    InputEvent::PressEnter { .. } | InputEvent::Focus => {}
                }
            }),
        ];
        let mut this = Self {
            ai,
            settings,
            form: None,
            removing: None,
            error: None,
            exclude,
            exclude_edited: false,
            _subscriptions: subscriptions,
        };
        this.sync_exclude(window, cx);
        this
    }

    /// The settings are read-only (DATA-1).
    fn read_only(&self, cx: &App) -> bool {
        self.settings.read(cx).read_only()
    }

    /// The open form's key field, while it is shown (SEC-1).
    pub fn key_field(&self, cx: &App) -> Option<Entity<InputState>> {
        let form = self.form.as_ref()?.form.read(cx);
        form.key_field().cloned()
    }

    /// Empties the open form's key field, as the dialog closes (SEC-1).
    pub fn clear_key(&self, window: &mut Window, cx: &mut App) {
        if let Some(open) = &self.form {
            open.form.update(cx, |form, cx| form.clear_key(window, cx));
        }
    }

    /// Shows the core's exclude list, unless the user is editing it.
    fn sync_exclude(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.exclude_edited {
            return;
        }
        let Some(list) = self.ai.read(cx).view().map(|view| view.exclude.join("\n")) else {
            return;
        };
        if self.exclude.read(cx).value() != list {
            self.exclude
                .update(cx, |exclude, cx| exclude.set_value(list, window, cx));
            // Setting it is no edit of the user's.
            self.exclude_edited = false;
        }
    }

    /// The exclude list as edited, once: it goes with the next save.
    fn take_exclude(&mut self, cx: &App) -> Option<Vec<String>> {
        if !self.exclude_edited {
            return None;
        }
        self.exclude_edited = false;
        Some(
            self.exclude
                .read(cx)
                .value()
                .lines()
                .map(str::to_owned)
                .collect(),
        )
    }

    fn save_exclude(&mut self, cx: &mut Context<Self>) {
        if self.exclude_edited {
            self.save_options(None, None, cx);
        }
    }

    /// Saves the switches given, with the exclude list when it was edited,
    /// so one save never puts back what another changed.
    fn save_options(
        &mut self,
        enabled: Option<bool>,
        debug_log: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let exclude = self.take_exclude(cx);
        let request = self
            .ai
            .update(cx, |ai, cx| ai.set_options(enabled, exclude, debug_log, cx));
        self.send(request, cx);
    }

    /// Sends `request`; its refusal shows in the tab.
    fn send(&mut self, request: Task<Result<(), SharedString>>, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            if let Err(error) = request.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn open_form(
        &mut self,
        saved: Option<ProviderView>,
        start: Option<DetectedCli>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ai = self.ai.clone();
        let form = cx.new(|cx| ProviderForm::new(ai, saved, start, window, cx));
        let done = cx.subscribe_in(&form, window, |this, _, _: &FormDone, _, cx| {
            this.form = None;
            cx.notify();
        });
        self.removing = None;
        self.error = None;
        // The form takes the list's place; the focus goes into it once it
        // is drawn.
        let first = form.downgrade();
        cx.defer_in(window, move |_, window, cx| {
            if let Some(form) = first.upgrade() {
                form.update(cx, |form, cx| form.focus_first(window, cx));
            }
        });
        self.form = Some(OpenForm { form, _done: done });
        cx.notify();
    }

    fn provider_row(
        &self,
        at: usize,
        provider: &ProviderView,
        is_default: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = *cx.tokens();
        let read_only = self.read_only(cx);
        let check = provider
            .kind
            .is_cli()
            .then(|| provider.check.as_ref().map(self_test_text))
            .flatten();
        let line = text::provider_line(
            &provider.name,
            is_default,
            provider.kind,
            provider.model.as_deref(),
            check.as_deref(),
        );
        let details: SharedString = line
            .strip_prefix(provider.name.as_str())
            .unwrap_or_default()
            .to_owned()
            .into();
        let name: SharedString = provider.name.clone().into();
        let actions = if self.removing.as_ref() == Some(&provider.id) {
            let id = provider.id.clone();
            let question: SharedString = text::remove_with_key(&provider.name).into();
            div()
                .id(("ai-provider-removing", at))
                .test_support()
                .role(Role::Group)
                .aria_label(question.clone())
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(question)
                .child(
                    Button::new(("ai-provider-remove-cancel", at))
                        .label(common::CANCEL)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.removing = None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(("ai-provider-remove-confirm", at))
                        .danger()
                        .label(text::REMOVE_CONFIRM)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.removing = None;
                            let id = id.clone();
                            let request = this.ai.update(cx, |ai, cx| ai.delete(id, cx));
                            this.send(request, cx);
                        })),
                )
                .into_any_element()
        } else {
            let (edit, default, remove) =
                (provider.clone(), provider.id.clone(), provider.id.clone());
            div()
                .flex()
                .flex_row()
                .gap_1()
                .child(
                    Button::new(("ai-provider-edit", at))
                        .label(text::EDIT)
                        .accessibility_label(text::edit_label(&provider.name))
                        .disabled(read_only)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_form(Some(edit.clone()), None, window, cx);
                        })),
                )
                .child(
                    Button::new(("ai-provider-default", at))
                        .label(text::SET_DEFAULT)
                        .accessibility_label(text::set_default_label(&provider.name))
                        .disabled(is_default || read_only)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = default.clone();
                            let request = this.ai.update(cx, |ai, cx| ai.set_default(id, cx));
                            this.send(request, cx);
                        })),
                )
                .child(
                    Button::new(("ai-provider-remove", at))
                        .label(text::REMOVE)
                        .accessibility_label(text::remove_label(&provider.name))
                        .disabled(read_only)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.removing = Some(remove.clone());
                            cx.notify();
                        })),
                )
                .into_any_element()
        };
        div()
            .id(("ai-provider", at))
            .test_support()
            .role(Role::ListItem)
            .aria_label(line)
            .flex()
            .flex_col()
            .gap_1()
            .py_1()
            .border_b_1()
            .border_color(tokens.border)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .child(
                        // The name as text, never markup (SEC-3).
                        div()
                            .id(("ai-provider-name", at))
                            .test_support()
                            .aria_label(name.clone())
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(div().text_color(tokens.text_secondary).child(details)),
            )
            .child(actions)
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let read_only = self.read_only(cx);
        let Some(view) = self.ai.read(cx).view().cloned() else {
            return Vec::new();
        };
        let rows: Vec<AnyElement> = view
            .providers
            .iter()
            .enumerate()
            .map(|(at, provider)| {
                let is_default = view.default.as_ref() == Some(&provider.id);
                self.provider_row(at, provider, is_default, cx)
            })
            .collect();
        let mut parts = vec![
            div()
                .id("ai-providers-heading")
                .test_support()
                .role(Role::Heading)
                .aria_level(3)
                .aria_label(text::PROVIDERS)
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(text::PROVIDERS)
                .into_any_element(),
        ];
        if view.providers.is_empty() {
            parts.push(note("ai-no-provider", Role::Paragraph, text::NO_PROVIDER));
        } else {
            parts.push(
                div()
                    .id("ai-providers")
                    .test_support()
                    .role(Role::List)
                    .aria_label(text::PROVIDERS_LABEL)
                    .flex()
                    .flex_col()
                    .children(rows)
                    .into_any_element(),
            );
        }
        if let Some(offer) = view.offer.clone() {
            let found: SharedString = text::found(
                text::kind_label(offer.kind),
                &offer.path.display().to_string(),
            )
            .into();
            parts.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("ai-offer")
                            .test_support()
                            .role(Role::Paragraph)
                            .aria_label(found.clone())
                            // A long path wraps beside the button instead
                            // of pushing it out of the dialog (UI-26).
                            .flex_1()
                            .min_w_0()
                            .child(found),
                    )
                    .child(
                        Button::new("ai-offer-add")
                            .flex_shrink_0()
                            .label(text::ADD_IT)
                            .disabled(read_only)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_form(None, Some(offer.clone()), window, cx);
                            })),
                    )
                    .into_any_element(),
            );
        }
        parts.push(
            div()
                .child(
                    Button::new("ai-add")
                        .label(text::ADD_PROVIDER)
                        .disabled(read_only)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_form(None, None, window, cx);
                        })),
                )
                .into_any_element(),
        );
        parts
    }
}

fn note(id: &'static str, role: Role, note: impl Into<SharedString>) -> AnyElement {
    let note = note.into();
    div()
        .id(id)
        .test_support()
        .role(role)
        .aria_label(note.clone())
        .child(note)
        .into_any_element()
}

impl Render for AiSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.ai.read(cx).view().cloned();
        let read_only = self.read_only(cx);
        let mut section = div()
            .id("ai-section")
            .test_support()
            .role(Role::Group)
            .aria_label(text::LEGEND)
            .flex()
            .flex_col()
            .gap_3();
        let Some(view) = view else {
            return section;
        };
        section = section.child(
            Checkbox::new("ai-features")
                .label(text::FEATURES)
                .checked(view.enabled)
                .disabled(read_only)
                .on_click(cx.listener(|this, value: &bool, _, cx| {
                    this.save_options(Some(*value), None, cx);
                })),
        );
        if !view.enabled {
            return section
                .child(note("ai-off-note", Role::Paragraph, text::OFF))
                .children(self.error.clone().map(|error| alert(error, cx)));
        }
        if let Some(reason) = &view.keychain_error {
            section = section.child(note(
                "ai-keychain-error",
                Role::Alert,
                text::no_keychain(reason),
            ));
        }
        section = match &self.form {
            Some(open) => section.child(open.form.clone()),
            None => section.children(self.list(cx)),
        };
        section
            .children(self.error.clone().map(|error| alert(error, cx)))
            .child(
                div().flex().flex_col().gap_1().child(text::EXCLUDE).child(
                    div().id("ai-exclude").test_support().h(px(96.)).child(
                        crate::text_area::tab_moves_on(
                            Textarea::new(&self.exclude)
                                .h(gpui_kit::relative(1.))
                                .aria_label(text::EXCLUDE)
                                .disabled(read_only),
                        )
                        .size_full(),
                    ),
                ),
            )
            .child(
                Checkbox::new("ai-debug-log")
                    .label(text::DEBUG_LOG)
                    .checked(view.debug_log)
                    .disabled(read_only)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.save_options(None, Some(*value), cx);
                    })),
            )
            .child(note(
                "ai-debug-log-note",
                Role::Paragraph,
                text::DEBUG_LOG_NOTE,
            ))
    }
}

fn alert(error: SharedString, cx: &App) -> AnyElement {
    let color = cx.tokens().danger;
    div()
        .id("ai-error")
        .test_support()
        .role(Role::Alert)
        .aria_label(error.clone())
        .text_color(color)
        .child(error)
        .into_any_element()
}
