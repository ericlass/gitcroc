//! The AI settings as the core reports them, a port of
//! rc.1's `ui/src/stores/ai.ts`: read with `ai_providers_get` at start, then kept
//! current by `AiChanged`, with each model list and self-test the user
//! asked for from `AiModels` and `AiSelfTest`. It keeps no copy of its own
//! (ARCH-6) and never a key (SEC-1): a typed key is handed straight to the
//! call that takes it.

use std::collections::HashMap;

use gitcroc_app::ai::{AiRequestId, AiView, ModelsResult, SelfTest};
use gitcroc_app::app::AppEvent;
use gitcroc_core::{AiOptions, ProviderDraft, ProviderId};
use gpui_kit::{App, Context, SharedString, Task};

use crate::services::Services;
use crate::workspaces::refusal;

pub struct Ai {
    services: Services,
    /// The core's view; `None` until the first read.
    view: Option<AiView>,
    /// An `AiChanged` has arrived, so the first read is older.
    heard: bool,
    /// The model lists that arrived, by request.
    models: HashMap<AiRequestId, ModelsResult>,
    /// The self-tests that ended, by request.
    tests: HashMap<AiRequestId, SelfTest>,
}

impl Ai {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            services,
            view: None,
            heard: false,
            models: HashMap::new(),
            tests: HashMap::new(),
        };
        this.read_again(cx);
        this
    }

    /// Reads the core's view; an event that arrives first is newer.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        self.heard = false;
        let call = self
            .services
            .with_app(|app| async move { app.ai_providers_get().await });
        cx.spawn(async move |this, cx| {
            if let Some(Ok(view)) = call.await {
                let _ = this.update(cx, |this, cx| {
                    if !this.heard {
                        this.view = Some(view);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::AiChanged(view) => {
                self.heard = true;
                self.view = Some(view.clone());
            }
            AppEvent::AiModels(reply) => {
                self.models.insert(reply.request_id, reply.result.clone());
            }
            AppEvent::AiSelfTest(reply) => {
                self.tests.insert(reply.request_id, reply.result.clone());
            }
            _ => return,
        }
        cx.notify();
    }

    pub fn view(&self) -> Option<&AiView> {
        self.view.as_ref()
    }

    /// AI features (AI-16): on until the core says otherwise.
    pub fn enabled(&self) -> bool {
        self.view.as_ref().is_none_or(|view| view.enabled)
    }

    /// The default provider's name, as Cancel shows it while its job runs.
    pub fn default_name(&self) -> String {
        self.view
            .as_ref()
            .and_then(|view| {
                let id = view.default.as_ref()?;
                view.providers.iter().find(|provider| &provider.id == id)
            })
            .map_or_else(
                || crate::strings::conflict::THE_PROVIDER.to_owned(),
                |provider| provider.name.clone(),
            )
    }

    /// The model list of `request`, once it arrived.
    pub fn models(&self, request: AiRequestId) -> Option<&ModelsResult> {
        self.models.get(&request)
    }

    /// The self-test of `request`, once it ended.
    pub fn test(&self, request: AiRequestId) -> Option<&SelfTest> {
        self.tests.get(&request)
    }

    /// Changes the switch, the exclude list and debug logging; the parts
    /// not given keep the core's values.
    pub fn set_options(
        &self,
        enabled: Option<bool>,
        exclude: Option<Vec<String>>,
        debug_log: Option<bool>,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        let Some(view) = &self.view else {
            return Task::ready(Ok(()));
        };
        let options = AiOptions {
            enabled: enabled.unwrap_or(view.enabled),
            exclude: exclude.unwrap_or_else(|| view.exclude.clone()),
            debug_log: debug_log.unwrap_or(view.debug_log),
        };
        self.send(
            move |app| async move { app.ai_settings_set(options).await },
            cx,
        )
    }

    /// Saves `draft`; `key` as `ai_provider_save` takes it.
    pub fn save(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        self.send(
            move |app| async move { app.ai_provider_save(draft, key).await },
            cx,
        )
    }

    pub fn delete(&self, id: ProviderId, cx: &mut App) -> Task<Result<(), SharedString>> {
        self.send(
            move |app| async move { app.ai_provider_delete(id).await },
            cx,
        )
    }

    pub fn set_default(&self, id: ProviderId, cx: &mut App) -> Task<Result<(), SharedString>> {
        self.send(
            move |app| async move { app.ai_provider_set_default(id).await },
            cx,
        )
    }

    /// Keeps the consent to `target`, the host or program the consent
    /// dialog showed (AI-6).
    pub fn consent(
        &self,
        id: ProviderId,
        target: String,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>> {
        self.send(
            move |app| async move { app.ai_provider_consent(id, target).await },
            cx,
        )
    }

    /// Asks for `draft`'s models; the list arrives as `AiModels`.
    pub fn fetch_models(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
        cx: &mut App,
    ) -> Task<Result<AiRequestId, SharedString>> {
        let call = self
            .services
            .with_app(move |app| async move { app.ai_models_get(draft, key).await });
        cx.spawn(async move |_| requested(call.await))
    }

    /// Tests `draft`; the result arrives as `AiSelfTest`.
    pub fn self_test(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
        cx: &mut App,
    ) -> Task<Result<AiRequestId, SharedString>> {
        let call = self
            .services
            .with_app(move |app| async move { app.ai_self_test(draft, key).await });
        cx.spawn(async move |_| requested(call.await))
    }

    fn send<F, T>(
        &self,
        call: impl FnOnce(gitcroc_app::app::App) -> F,
        cx: &mut App,
    ) -> Task<Result<(), SharedString>>
    where
        F: Future<Output = Result<T, gitcroc_app::app::AppError>> + Send + 'static,
        T: Send + 'static,
    {
        let call = self.services.with_app(call);
        cx.spawn(async move |_| refusal(call.await))
    }
}

fn requested(
    result: Option<Result<AiRequestId, gitcroc_app::app::AppError>>,
) -> Result<AiRequestId, SharedString> {
    match result {
        Some(Ok(id)) => Ok(id),
        Some(Err(error)) => Err(error.message.into()),
        // The runtime is shutting down.
        None => Err(SharedString::default()),
    }
}
