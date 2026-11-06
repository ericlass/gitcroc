//! One AI provider's form in Settings, a port of rc.1's `ui/src/ai/ProviderForm.tsx`
//! (AI-1, AI-2, AI-10): the kind, from the core's list only, then the
//! fields section 7 gives that kind and the context budget; models fetched
//! from the provider, none preselected, or typed; and Test. The key field
//! is write-only (SEC-1): it says "Key stored" and never shows a key, can
//! replace or remove it, and a typed key goes only to the calls that take
//! it and is cleared when the form closes. The core checks and saves; its
//! refusal shows here, and nothing is saved then.

use std::path::PathBuf;

use gitcroc_app::ai::{AiRequestId, DetectedCli, ModelsResult, ProviderView, SelfTest};
use gitcroc_core::{ProviderDraft, ProviderKind};
use gpui_component::Disableable;
use gpui_component::Selectable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, TestSupportExt, Window, div,
};

use crate::ai::Ai;
use crate::dialog;
use crate::strings::{ai as text, common};
use crate::theme::ActiveTokens;

/// The form is done: saved or cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormDone;

/// The kinds that call an API, which may have a key and need a model; the
/// CLI kinds and the mock take neither.
fn is_api(kind: ProviderKind) -> bool {
    matches!(kind, ProviderKind::Anthropic | ProviderKind::Openai)
}

/// A self-test's state in words (AI-2).
pub fn self_test_text(test: &SelfTest) -> String {
    match test {
        SelfTest::Running => text::TESTING.to_owned(),
        SelfTest::Passed { version: None } => text::WORKS.to_owned(),
        SelfTest::Passed {
            version: Some(version),
        } => text::works_with(version),
        SelfTest::MissingFlags { version, missing } => text::lacks(version, &missing.join(", ")),
        SelfTest::Failed { reason } => text::failed(reason),
    }
}

pub struct ProviderForm {
    ai: Entity<Ai>,
    /// The provider as it was when the form opened; `None` adds one.
    saved: Option<ProviderView>,
    kind: ProviderKind,
    name: Entity<InputState>,
    base_url: Entity<InputState>,
    /// A key typed now; sent once and emptied when the form closes.
    key: Entity<InputState>,
    path: Entity<InputState>,
    model: Entity<InputState>,
    budget: Entity<InputState>,
    replacing: bool,
    remove_key: bool,
    models_request: Option<AiRequestId>,
    test_request: Option<AiRequestId>,
    /// A call to the core is on its way.
    sending: bool,
    error: Option<SharedString>,
    /// The first control: the first kind, or the name when editing.
    first: FocusHandle,
    _ai_changed: Subscription,
}

impl EventEmitter<FormDone> for ProviderForm {}

impl ProviderForm {
    /// The form for `saved`, or for a new provider, started from `start`
    /// when the core offered a detected CLI.
    pub fn new(
        ai: Entity<Ai>,
        saved: Option<ProviderView>,
        start: Option<DetectedCli>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kind = saved
            .as_ref()
            .map(|saved| saved.kind)
            .or(start.as_ref().map(|start| start.kind))
            .unwrap_or(ProviderKind::Anthropic);
        let name = saved.as_ref().map_or_else(
            || {
                start
                    .as_ref()
                    .map(|start| text::kind_label(start.kind).to_owned())
                    .unwrap_or_default()
            },
            |saved| saved.name.clone(),
        );
        let path = saved
            .as_ref()
            .and_then(|saved| saved.path.clone())
            .or(start.map(|start| start.path))
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let field = |value: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let name = field(name, window, cx);
        let base_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(text::BASE_URL_PLACEHOLDER)
                .default_value(
                    saved
                        .as_ref()
                        .and_then(|saved| saved.base_url.clone())
                        .unwrap_or_default(),
                )
        });
        let key = cx.new(|cx| InputState::new(window, cx).masked(true));
        let path = field(path, window, cx);
        let model = field(
            saved
                .as_ref()
                .and_then(|saved| saved.model.clone())
                .unwrap_or_default(),
            window,
            cx,
        );
        let budget = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(text::CONTEXT_BUDGET_PLACEHOLDER)
                .default_value(
                    saved
                        .as_ref()
                        .map(|saved| saved.context_budget.to_string())
                        .unwrap_or_default(),
                )
        });
        let ai_changed = cx.observe(&ai, |_, _, cx| cx.notify());
        Self {
            ai,
            saved,
            kind,
            name,
            base_url,
            key,
            path,
            model,
            budget,
            replacing: false,
            remove_key: false,
            models_request: None,
            test_request: None,
            sending: false,
            error: None,
            first: cx.focus_handle(),
            _ai_changed: ai_changed,
        }
    }

    /// Gives the focus to the first control.
    pub fn focus_first(&self, window: &mut Window, cx: &mut App) {
        if self.saved.is_none() {
            window.focus(&self.first, cx);
        } else {
            self.name.update(cx, |name, cx| name.focus(window, cx));
        }
    }

    /// The key field, while it is shown.
    pub fn key_field(&self) -> Option<&Entity<InputState>> {
        self.key_shown().then_some(&self.key)
    }

    /// Empties the key field (SEC-1).
    pub fn clear_key(&self, window: &mut Window, cx: &mut App) {
        self.key.update(cx, |key, cx| key.set_value("", window, cx));
    }

    fn has_key(&self) -> bool {
        self.saved
            .as_ref()
            .is_some_and(|saved| saved.has_key && saved.kind == self.kind)
            && !self.remove_key
    }

    fn key_shown(&self) -> bool {
        is_api(self.kind) && !(self.has_key() && !self.replacing)
    }

    fn value(field: &Entity<InputState>, cx: &App) -> String {
        field.read(cx).value().to_string()
    }

    fn draft(&self, cx: &App) -> ProviderDraft {
        let model = Self::value(&self.model, cx);
        let model = model.trim();
        ProviderDraft {
            id: self.saved.as_ref().map(|saved| saved.id.clone()),
            name: Self::value(&self.name, cx),
            kind: self.kind,
            model: (!model.is_empty()).then(|| model.to_owned()),
            base_url: (self.kind == ProviderKind::Openai).then(|| Self::value(&self.base_url, cx)),
            path: self
                .kind
                .is_cli()
                .then(|| PathBuf::from(Self::value(&self.path, cx))),
            context_budget: Self::value(&self.budget, cx).trim().parse().ok(),
        }
    }

    /// What the core gets for the key: `None` keeps the stored one, an
    /// empty one removes it.
    fn key_to_send(&self, cx: &App) -> Option<String> {
        if !is_api(self.kind) {
            return None;
        }
        if self.remove_key {
            return Some(String::new());
        }
        let key = Self::value(&self.key, cx);
        (!key.trim().is_empty()).then_some(key)
    }

    fn models(&self, cx: &App) -> Option<ModelsResult> {
        let request = self.models_request?;
        self.ai.read(cx).models(request).cloned()
    }

    fn fetching(&self, cx: &App) -> bool {
        self.models_request.is_some() && self.models(cx).is_none()
    }

    /// The test run here, else a CLI provider's check from the core.
    fn test_result(&self, cx: &App) -> Option<SelfTest> {
        if let Some(request) = self.test_request {
            return Some(
                self.ai
                    .read(cx)
                    .test(request)
                    .cloned()
                    .unwrap_or(SelfTest::Running),
            );
        }
        self.saved
            .as_ref()
            .filter(|saved| saved.kind == self.kind)
            .and_then(|saved| saved.check.clone())
    }

    fn testing(&self, cx: &App) -> bool {
        matches!(self.test_result(cx), Some(SelfTest::Running)) && self.test_request.is_some()
    }

    /// A model list or a test is asked for: their buttons are busy.
    fn busy(&self, cx: &App) -> bool {
        self.sending || self.fetching(cx) || self.testing(cx)
    }

    /// The offered models, with the chosen one first when the list lacks
    /// it; none is preselected (AI-1).
    fn offered(&self, cx: &App) -> Option<Vec<String>> {
        let Some(ModelsResult::Offered { models }) = self.models(cx) else {
            return None;
        };
        let current = Self::value(&self.model, cx).trim().to_owned();
        if current.is_empty() || models.contains(&current) {
            Some(models)
        } else {
            Some(std::iter::once(current).chain(models).collect())
        }
    }

    fn set_kind(&mut self, kind: ProviderKind, cx: &mut Context<Self>) {
        self.kind = kind;
        self.models_request = None;
        self.test_request = None;
        cx.notify();
    }

    fn fetch_models(&mut self, cx: &mut Context<Self>) {
        if self.busy(cx) {
            return;
        }
        let (draft, key) = (self.draft(cx), self.key_to_send(cx));
        let request = self.ai.update(cx, |ai, cx| ai.fetch_models(draft, key, cx));
        self.models_request = None;
        self.ask(request, |this, id| this.models_request = Some(id), cx);
    }

    fn self_test(&mut self, cx: &mut Context<Self>) {
        if self.busy(cx) {
            return;
        }
        let (draft, key) = (self.draft(cx), self.key_to_send(cx));
        let request = self.ai.update(cx, |ai, cx| ai.self_test(draft, key, cx));
        self.test_request = None;
        self.ask(request, |this, id| this.test_request = Some(id), cx);
    }

    /// Waits for the core to take `request`; its refusal shows in the form.
    fn ask(
        &mut self,
        request: Task<Result<AiRequestId, SharedString>>,
        asked: impl FnOnce(&mut Self, AiRequestId) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.sending = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |this, cx| {
                this.sending = false;
                match result {
                    Ok(id) => asked(this, id),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        let (draft, key) = (self.draft(cx), self.key_to_send(cx));
        let request = self.ai.update(cx, |ai, cx| ai.save(draft, key, cx));
        self.sending = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = request.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Ok(()) => this.done(window, cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_key(window, cx);
        cx.emit(FormDone);
    }

    fn field(
        id: &'static str,
        label: impl Into<SharedString>,
        input: &Entity<InputState>,
    ) -> AnyElement {
        let label = label.into();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label.clone())
            .child(Input::new(input).id(id).aria_label(label))
            .into_any_element()
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

    fn kind_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let kinds = self
            .ai
            .read(cx)
            .view()
            .map(|view| view.kinds.clone())
            .unwrap_or_default();
        let buttons = kinds.into_iter().enumerate().map(|(at, kind)| {
            Button::new(("provider-kind", at))
                .label(text::kind_label(kind))
                .selected(self.kind == kind)
                .role(Role::RadioButton)
                .toggled(self.kind == kind)
                .when(at == 0, |button| button.track_focus(&self.first))
                .on_click(cx.listener(move |this, _, _, cx| this.set_kind(kind, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(text::KIND)
            .child(
                div()
                    .id("provider-kinds")
                    .test_support()
                    .role(Role::RadioGroup)
                    .aria_label(text::KIND)
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(buttons),
            )
            .into_any_element()
    }

    fn key_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut group = div()
            .id("provider-key")
            .test_support()
            .role(Role::Group)
            .aria_label(text::API_KEY)
            .flex()
            .flex_col()
            .gap_1();
        if self.key_shown() {
            let label = if self.kind == ProviderKind::Openai {
                text::API_KEY_OPTIONAL
            } else {
                text::API_KEY
            };
            group = group.child(Self::field("provider-key-field", label, &self.key));
        } else {
            group = group.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Self::note(
                        "provider-key-stored",
                        Role::Paragraph,
                        text::KEY_STORED,
                    ))
                    .child(
                        Button::new("provider-key-replace")
                            .label(text::REPLACE_KEY)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.replacing = true;
                                this.key.update(cx, |key, cx| key.focus(window, cx));
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("provider-key-remove")
                            .label(text::REMOVE_KEY)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.remove_key = true;
                                this.clear_key(window, cx);
                                cx.notify();
                            })),
                    ),
            );
        }
        group
            .when(self.remove_key, |group| {
                group.child(Self::note(
                    "provider-key-removed",
                    Role::Paragraph,
                    text::KEY_REMOVED_ON_SAVE,
                ))
            })
            .into_any_element()
    }

    fn cli_fields(&self, cx: &mut Context<Self>) -> AnyElement {
        let detected: Vec<PathBuf> = self
            .ai
            .read(cx)
            .view()
            .map(|view| {
                view.detected
                    .iter()
                    .filter(|cli| cli.kind == self.kind)
                    .map(|cli| cli.path.clone())
                    .collect()
            })
            .unwrap_or_default();
        let current = Self::value(&self.path, cx);
        let choices = detected.into_iter().enumerate().map(|(at, path)| {
            let shown = path.display().to_string();
            Button::new(("provider-detected-path", at))
                .label(shown.clone())
                .selected(current == shown)
                .role(Role::RadioButton)
                .toggled(current == shown)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.path
                        .update(cx, |path, cx| path.set_value(shown.clone(), window, cx));
                    cx.notify();
                }))
        });
        let choices: Vec<_> = choices.collect();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(!choices.is_empty(), |this| {
                this.child(
                    div()
                        .id("provider-detected")
                        .test_support()
                        .role(Role::RadioGroup)
                        .aria_label(text::PROGRAMS_FOUND)
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(text::PROGRAMS_FOUND)
                        .children(choices),
                )
            })
            .child(Self::field("provider-path", text::PROGRAM, &self.path))
            .into_any_element()
    }

    fn model_fields(&self, cx: &mut Context<Self>) -> AnyElement {
        let label = if is_api(self.kind) {
            text::MODEL
        } else {
            text::MODEL_OPTIONAL
        };
        let field = Self::field("provider-model-field", label, &self.model);
        if let Some(models) = self.offered(cx) {
            let current = Self::value(&self.model, cx);
            let current = current.trim();
            let choices = models.into_iter().enumerate().map(|(at, model)| {
                Button::new(("provider-model", at))
                    .label(model.clone())
                    .selected(current == model)
                    .role(Role::RadioButton)
                    .toggled(current == model)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.model
                            .update(cx, |field, cx| field.set_value(model.clone(), window, cx));
                        cx.notify();
                    }))
            });
            return div()
                .id("provider-models")
                .test_support()
                .role(Role::RadioGroup)
                .aria_label(text::MODEL)
                .flex()
                .flex_col()
                .gap_1()
                .child(text::MODEL)
                .when(current.is_empty(), |this| {
                    this.child(Self::note(
                        "provider-pick-model",
                        Role::Paragraph,
                        text::PICK_MODEL,
                    ))
                })
                .children(choices)
                .into_any_element();
        }
        let note = if self.fetching(cx) {
            Some(Self::note(
                "provider-models-fetching",
                Role::Status,
                text::FETCHING_MODELS,
            ))
        } else {
            match self.models(cx) {
                Some(ModelsResult::NotOffered) => Some(Self::note(
                    "provider-models-none",
                    Role::Paragraph,
                    text::NO_MODEL_LIST,
                )),
                Some(ModelsResult::Failed { reason }) => Some(Self::note(
                    "provider-models-failed",
                    Role::Alert,
                    text::cannot_fetch_models(&reason),
                )),
                _ => None,
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(field)
            .children(note)
            .into_any_element()
    }
}

impl Render for ProviderForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let title: SharedString = self
            .saved
            .as_ref()
            .map_or_else(
                || text::NEW_PROVIDER.to_owned(),
                |saved| text::provider(&saved.name),
            )
            .into();
        let busy = self.busy(cx);
        let kind = self.kind;
        let test = self.test_result(cx).map(|test| {
            Self::note(
                "provider-test-result",
                Role::Status,
                text::test_result(&self_test_text(&test)),
            )
        });
        let error = self.error.clone().map(|error| {
            div()
                .id("provider-error")
                .test_support()
                .role(Role::Alert)
                .aria_label(error.clone())
                .text_color(tokens.danger)
                .child(error)
        });
        let mut form = div()
            .id("provider-form")
            .test_support()
            .role(Role::Form)
            .aria_label(title)
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .border_1()
            .border_color(tokens.border)
            .rounded_md();
        form = if self.saved.is_none() {
            form.child(self.kind_choices(cx))
        } else {
            form.child(Self::note(
                "provider-kind-name",
                Role::Paragraph,
                text::kind_label(kind),
            ))
        };
        form = form.child(Self::field("provider-name", text::NAME, &self.name));
        if kind == ProviderKind::Openai {
            form = form.child(Self::field(
                "provider-base-url",
                text::BASE_URL,
                &self.base_url,
            ));
        }
        if is_api(kind) {
            form = form.child(self.key_group(cx));
        }
        if kind.is_cli() {
            form = form.child(self.cli_fields(cx));
        }
        form.child(self.model_fields(cx))
            .child(Self::field(
                "provider-budget",
                text::CONTEXT_BUDGET,
                &self.budget,
            ))
            .children(test)
            .children(error)
            .child(
                dialog::actions_row()
                    .when(is_api(kind), |row| {
                        row.child(
                            Button::new("provider-fetch-models")
                                .label(text::FETCH_MODELS)
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.fetch_models(cx))),
                        )
                    })
                    .child(
                        Button::new("provider-test")
                            .label(text::TEST)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.self_test(cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("provider-cancel")
                            .label(common::CANCEL)
                            .on_click(cx.listener(|this, _, window, cx| this.done(window, cx))),
                    )
                    .child(
                        Button::new("provider-save")
                            .primary()
                            .label(text::SAVE)
                            .disabled(self.sending)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
