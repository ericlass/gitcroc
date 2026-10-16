//! The AI providers behind the IPC (M4-05): the entries in `settings.toml`
//! through the core, their keys in the OS keychain (SEC-1, D11), their
//! models (AI-1), and the CLI providers' detection and self-test (AI-2),
//! all of which stop while AI features are off (AI-16). No key ever leaves
//! this module toward the UI: views say only whether a key is stored.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_ai::{
    AnthropicProvider, ApiKey, ClaudeCodeProvider, CodexProvider, ModelList, OpenAiProvider,
    SelfTestError,
};
use gitcroc_core::{
    AiOptions, AiSettingsError, AppLog, DEFAULT_CONTEXT_BUDGET, ProviderDraft, ProviderEntry,
    ProviderId, ProviderKind, SettingsError, SettingsStore,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::platform::{Keychain, KeychainError, keychain_account};

/// The future an [`AiBackend`] call returns.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A CLI found by detection (AI-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DetectedCli {
    pub kind: ProviderKind,
    pub path: PathBuf,
}

/// A self-test's state or result (AI-2). API providers are tested by
/// listing their models; a CLI provider by its version and flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelfTest {
    Running,
    /// `version` is the CLI's; API providers have none.
    Passed {
        version: Option<String>,
    },
    /// The installed CLI lacks flags the provider uses.
    MissingFlags {
        version: String,
        missing: Vec<String>,
    },
    Failed {
        reason: String,
    },
}

impl SelfTest {
    /// Why the provider cannot be used, if it cannot.
    fn problem(&self) -> Option<String> {
        match self {
            Self::Running | Self::Passed { .. } => None,
            Self::MissingFlags { version, missing } => {
                Some(format!("{version} lacks {}", missing.join(", ")))
            }
            Self::Failed { reason } => Some(reason.clone()),
        }
    }
}

/// The models a provider offers (AI-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelsResult {
    /// In the provider's order.
    Offered {
        models: Vec<String>,
    },
    /// The provider has no list; the user types the name.
    NotOffered,
    Failed {
        reason: String,
    },
}

/// Identifies a models or self-test request, unique while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct AiRequestId(pub u32);

/// The answer to [`AiProviders::models`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AiModelsReply {
    pub request_id: AiRequestId,
    pub result: ModelsResult,
}

/// The answer to [`AiProviders::self_test`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AiSelfTestReply {
    pub request_id: AiRequestId,
    pub result: SelfTest,
}

/// A configured provider as the UI sees it: its entry, without the file's
/// defaults, and never its key (SEC-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ProviderView {
    pub id: ProviderId,
    pub name: String,
    pub kind: ProviderKind,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub path: Option<PathBuf>,
    pub context_budget: u32,
    /// The host or program the user consented to (AI-6).
    pub consent: Option<String>,
    pub has_key: bool,
    /// A CLI provider's self-test while AI is on; `None` otherwise.
    pub check: Option<SelfTest>,
}

/// The AI settings and providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AiView {
    /// AI features (AI-16).
    pub enabled: bool,
    pub exclude: Vec<String>,
    pub debug_log: bool,
    pub default: Option<ProviderId>,
    pub providers: Vec<ProviderView>,
    /// Every CLI detection found while AI is on.
    pub detected: Vec<DetectedCli>,
    /// The first detected CLI that is not configured yet, offered to the
    /// user and never added on its own.
    pub offer: Option<DetectedCli>,
    /// Why the keychain could not be used, last time it was tried.
    pub keychain_error: Option<String>,
}

/// What reaches the providers: the real ones in the app, a fake in tests.
pub trait AiBackend: Send + Sync + 'static {
    /// Every installed Claude Code and Codex. Blocks briefly.
    fn detect(&self) -> Vec<DetectedCli>;

    /// Checks the CLI of `kind` at `path`.
    fn self_test<'a>(&'a self, kind: ProviderKind, path: &'a Path) -> BoxFuture<'a, SelfTest>;

    /// Lists the models of `entry`, which may be unsaved.
    fn list_models<'a>(
        &'a self,
        entry: &'a ProviderEntry,
        key: Option<ApiKey>,
    ) -> BoxFuture<'a, ModelsResult>;
}

/// Where the views and replies go: the window's events in the app.
pub trait AiSink: Send + Sync + 'static {
    fn changed(&self, view: &AiView);
    fn models(&self, reply: &AiModelsReply);
    fn self_test(&self, reply: &AiSelfTestReply);
}

/// Why a request was refused. It never holds a key (SEC-1).
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("AI features are off")]
    Off,
    #[error("{0}")]
    Settings(#[from] SettingsError),
    #[error("{0}")]
    Invalid(#[from] AiSettingsError),
    #[error("{0}")]
    Keychain(#[from] KeychainError),
    #[error("an Anthropic provider needs an API key")]
    NeedsKey,
    #[error("{0} providers use their own login and take no API key")]
    TakesNoKey(ProviderKind),
}

/// What a save does with the key: `None` keeps it, an empty one removes it.
enum KeyChange {
    Keep,
    Remove,
    Set(String),
}

impl From<Option<String>> for KeyChange {
    fn from(key: Option<String>) -> Self {
        match key {
            None => Self::Keep,
            Some(key) if key.trim().is_empty() => Self::Remove,
            Some(key) => Self::Set(key),
        }
    }
}

/// A CLI provider's latest self-test.
struct Check {
    /// Which run this is; a result from an older run is dropped.
    run: u64,
    state: SelfTest,
}

#[derive(Default)]
struct State {
    detected: Vec<DetectedCli>,
    /// Which detection may store its result; a stale one is dropped.
    detection_run: u64,
    checks: HashMap<ProviderId, Check>,
    runs: u64,
    /// Whether each provider has a key, as last read or written.
    has_key: HashMap<ProviderId, bool>,
    keychain_error: Option<String>,
    last_request: u32,
}

/// The configured providers, their keys and their checks, for the whole
/// app. The methods that are not `async` block on the keychain or the
/// settings file, so the IPC calls them off the async workers (ARCH-7).
pub struct AiProviders {
    settings: Arc<SettingsStore>,
    keychain: Arc<dyn Keychain>,
    backend: Arc<dyn AiBackend>,
    sink: Arc<dyn AiSink>,
    log: Arc<AppLog>,
    runtime: Handle,
    state: Mutex<State>,
    /// The background work started and not yet awaited by [`Self::settle`].
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change under these locks is one assignment or insertion, so a
    // panic cannot leave them half-changed.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AiProviders {
    pub fn new(
        settings: Arc<SettingsStore>,
        keychain: Arc<dyn Keychain>,
        backend: Arc<dyn AiBackend>,
        sink: Arc<dyn AiSink>,
        log: Arc<AppLog>,
        runtime: Handle,
    ) -> Arc<Self> {
        Arc::new(Self {
            settings,
            keychain,
            backend,
            sink,
            log,
            runtime,
            state: Mutex::new(State::default()),
            tasks: Mutex::new(Vec::new()),
        })
    }

    /// At app start: with AI on, detects the CLIs and self-tests every CLI
    /// provider in the background (AI-2). With AI off, nothing (AI-16).
    pub fn start(self: &Arc<Self>) {
        if self.settings.ai().enabled {
            self.refresh();
        }
    }

    /// Waits for every background task started so far, and the ones they
    /// start. For tests and shutdown; the app never needs to wait.
    pub async fn settle(&self) {
        loop {
            let tasks = std::mem::take(&mut *lock(&self.tasks));
            if tasks.is_empty() {
                return;
            }
            for task in tasks {
                // A task that panicked has nothing left to wait for.
                drop(task.await);
            }
        }
    }

    /// The AI settings and providers. Reads the keychain for the providers
    /// whose key state is not known yet.
    pub fn view(&self) -> AiView {
        let ai = self.settings.ai();
        let has_key: Vec<bool> = ai
            .providers
            .iter()
            .map(|entry| self.has_key(entry).unwrap_or(false))
            .collect();
        let state = lock(&self.state);
        let offer = state
            .detected
            .iter()
            .find(|cli| {
                !ai.providers
                    .iter()
                    .any(|entry| entry.path.as_ref() == Some(&cli.path))
            })
            .cloned();
        AiView {
            enabled: ai.enabled,
            default: ai.default,
            providers: ai
                .providers
                .into_iter()
                .zip(has_key)
                .map(|(entry, has_key)| ProviderView {
                    check: state.checks.get(&entry.id).map(|check| check.state.clone()),
                    id: entry.id,
                    name: entry.name,
                    kind: entry.kind,
                    model: entry.model,
                    base_url: entry.base_url,
                    path: entry.path,
                    context_budget: entry.context_budget,
                    consent: entry.consent,
                    has_key,
                })
                .collect(),
            offer,
            detected: state.detected.clone(),
            keychain_error: state.keychain_error.clone(),
            exclude: ai.exclude,
            debug_log: ai.debug_log,
        }
    }

    /// Saves `draft` with `key`: `None` keeps the stored key, an empty one
    /// removes it. A new provider starts without any key. The key is
    /// stored before the entry, so a keychain that refuses it leaves no
    /// entry behind; a CLI provider is self-tested again.
    pub fn save(
        self: &Arc<Self>,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<AiView, AiError> {
        let change = KeyChange::from(key);
        if draft.kind.is_cli() && matches!(change, KeyChange::Set(_)) {
            return Err(AiError::TakesNoKey(draft.kind));
        }
        let entry = self
            .settings
            .save_provider(draft, |entry, new| self.store_key(entry, new, &change))?;
        if !entry.kind.is_cli() {
            lock(&self.state).checks.remove(&entry.id);
        } else if self.settings.ai().enabled {
            self.check(&entry);
        }
        Ok(self.changed())
    }

    /// Removes the provider and its key (D11). A key the keychain cannot
    /// remove is logged and reported; a new provider that takes the same
    /// id removes it before it starts.
    pub fn delete(self: &Arc<Self>, id: &ProviderId) -> Result<AiView, AiError> {
        let entry = self.settings.delete_provider(id)?;
        if let Err(error) = self.keychain_call(|keychain| keychain.remove(&account(id))) {
            self.log.warn(&format!(
                "the key of the AI provider {} stayed in the keychain: {error}",
                entry.name
            ));
        }
        let mut state = lock(&self.state);
        state.has_key.remove(id);
        state.checks.remove(id);
        drop(state);
        Ok(self.changed())
    }

    /// Makes `id` the default provider (AI-1).
    pub fn set_default(&self, id: &ProviderId) -> Result<AiView, AiError> {
        self.settings.set_default_provider(id)?;
        Ok(self.changed())
    }

    /// Changes the AI features switch, the exclude list and the debug log
    /// switch. Turning AI on detects and self-tests as at start; turning it
    /// off drops what they found (AI-16).
    pub fn set_options(self: &Arc<Self>, options: AiOptions) -> Result<AiView, AiError> {
        let was = self.settings.ai().enabled;
        let ai = self.settings.set_ai_options(options)?;
        if ai.enabled && !was {
            self.refresh();
        } else if !ai.enabled {
            let mut state = lock(&self.state);
            state.detected.clear();
            state.detection_run += 1;
            state.checks.clear();
        }
        Ok(self.changed())
    }

    /// Lists the models of `draft`, saved or not, in the background; the
    /// reply goes to the sink. `key` is used as typed; without one, the
    /// stored key of the draft's id (AI-1).
    pub fn models(
        self: &Arc<Self>,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<AiRequestId, AiError> {
        let (entry, key) = self.prepare(draft, key)?;
        let request_id = self.next_request();
        let ai = self.clone();
        self.spawn(async move {
            let result = ai.backend.list_models(&entry, key).await;
            if ai.settings.ai().enabled {
                ai.sink.models(&AiModelsReply { request_id, result });
            }
        });
        Ok(request_id)
    }

    /// Tests `draft`, saved or not, in the background, as [`Self::models`]
    /// does: a CLI provider by its self-test, an API provider by listing
    /// its models (AI-2).
    pub fn self_test(
        self: &Arc<Self>,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<AiRequestId, AiError> {
        let (entry, key) = self.prepare(draft, key)?;
        let request_id = self.next_request();
        let ai = self.clone();
        self.spawn(async move {
            let result = match (entry.kind.is_cli(), &entry.path) {
                (true, Some(path)) => ai.backend.self_test(entry.kind, path).await,
                _ => match ai.backend.list_models(&entry, key).await {
                    ModelsResult::Offered { .. } | ModelsResult::NotOffered => {
                        SelfTest::Passed { version: None }
                    }
                    ModelsResult::Failed { reason } => SelfTest::Failed { reason },
                },
            };
            if ai.settings.ai().enabled {
                ai.sink.self_test(&AiSelfTestReply { request_id, result });
            }
        });
        Ok(request_id)
    }

    /// Refuses a request while AI is off, and a draft that cannot be
    /// reached; returns the entry to reach and its key.
    fn prepare(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<(ProviderEntry, Option<ApiKey>), AiError> {
        if !self.settings.ai().enabled {
            return Err(AiError::Off);
        }
        draft.check_connection()?;
        let key = match KeyChange::from(key) {
            _ if draft.kind.is_cli() => None,
            KeyChange::Set(key) => Some(ApiKey::new(key)),
            KeyChange::Remove => None,
            KeyChange::Keep => match &draft.id {
                Some(id) => self
                    .keychain_call(|keychain| keychain.read(&account(id)))?
                    .map(ApiKey::new),
                None => None,
            },
        };
        let entry = ProviderEntry {
            id: draft.id.unwrap_or_else(|| ProviderId(String::new())),
            name: draft.name,
            kind: draft.kind,
            model: draft.model,
            base_url: draft.base_url.map(|url| url.trim().to_owned()),
            path: draft.path,
            context_budget: draft.context_budget.unwrap_or(DEFAULT_CONTEXT_BUDGET),
            consent: None,
        };
        Ok((entry, key))
    }

    /// Applies `change` to the key of `entry`, before the entry is saved.
    fn store_key(
        &self,
        entry: &ProviderEntry,
        new: bool,
        change: &KeyChange,
    ) -> Result<(), AiError> {
        let account = account(&entry.id);
        let needs_key = entry.kind == ProviderKind::Anthropic;
        let has_key = match change {
            KeyChange::Set(key) => {
                self.keychain_call(|keychain| keychain.write(&account, key))?;
                true
            }
            KeyChange::Remove if needs_key => return Err(AiError::NeedsKey),
            KeyChange::Remove => {
                self.keychain_call(|keychain| keychain.remove(&account))?;
                false
            }
            KeyChange::Keep if new || entry.kind.is_cli() => {
                if needs_key {
                    return Err(AiError::NeedsKey);
                }
                // A key left behind by a deleted provider with this id, or
                // by this one before it became a CLI provider. The keychain
                // reports its own state if it fails.
                drop(self.keychain_call(|keychain| keychain.remove(&account)));
                false
            }
            KeyChange::Keep => {
                let has_key = self.has_key(entry)?;
                if needs_key && !has_key {
                    return Err(AiError::NeedsKey);
                }
                has_key
            }
        };
        lock(&self.state).has_key.insert(entry.id.clone(), has_key);
        Ok(())
    }

    /// Whether `entry` has a key, from memory or else from the keychain.
    /// CLI providers never have one.
    fn has_key(&self, entry: &ProviderEntry) -> Result<bool, KeychainError> {
        if entry.kind.is_cli() {
            return Ok(false);
        }
        if let Some(known) = lock(&self.state).has_key.get(&entry.id) {
            return Ok(*known);
        }
        let has_key = self
            .keychain_call(|keychain| keychain.read(&account(&entry.id)))?
            .is_some();
        lock(&self.state).has_key.insert(entry.id.clone(), has_key);
        Ok(has_key)
    }

    /// Runs a keychain call and keeps its state for the view; the first
    /// failure after a success is logged.
    fn keychain_call<T>(
        &self,
        call: impl FnOnce(&dyn Keychain) -> Result<T, KeychainError>,
    ) -> Result<T, KeychainError> {
        let result = call(self.keychain.as_ref());
        let mut state = lock(&self.state);
        match &result {
            Ok(_) => state.keychain_error = None,
            Err(error) => {
                if state.keychain_error.is_none() {
                    self.log.warn(&error.to_string());
                }
                state.keychain_error = Some(error.to_string());
            }
        }
        result
    }

    /// Detects the CLIs and self-tests every CLI provider, in the
    /// background.
    fn refresh(self: &Arc<Self>) {
        let run = {
            let mut state = lock(&self.state);
            state.detection_run += 1;
            state.detection_run
        };
        let ai = self.clone();
        self.spawn(async move {
            let backend = ai.backend.clone();
            let Ok(detected) = tokio::task::spawn_blocking(move || backend.detect()).await else {
                return;
            };
            {
                let mut state = lock(&ai.state);
                if state.detection_run != run {
                    return;
                }
                state.detected = detected;
            }
            ai.send_changed().await;
        });
        for entry in self.settings.ai().providers {
            if entry.kind.is_cli() {
                self.check(&entry);
            }
        }
    }

    /// Self-tests the CLI provider `entry` in the background; the result
    /// replaces any earlier one, and a failure is logged.
    fn check(self: &Arc<Self>, entry: &ProviderEntry) {
        let Some(path) = entry.path.clone() else {
            return;
        };
        let run = {
            let mut state = lock(&self.state);
            state.runs += 1;
            let run = state.runs;
            state.checks.insert(
                entry.id.clone(),
                Check {
                    run,
                    state: SelfTest::Running,
                },
            );
            run
        };
        let ai = self.clone();
        let (id, kind, name) = (entry.id.clone(), entry.kind, entry.name.clone());
        self.spawn(async move {
            let result = ai.backend.self_test(kind, &path).await;
            {
                let mut state = lock(&ai.state);
                match state.checks.get_mut(&id) {
                    Some(check) if check.run == run => check.state = result.clone(),
                    _ => return,
                }
            }
            if let Some(problem) = result.problem() {
                ai.log.warn(&format!(
                    "the AI provider {name} failed its self-test: {problem}"
                ));
            }
            ai.send_changed().await;
        });
    }

    /// Sends the view to the sink and returns it.
    fn changed(&self) -> AiView {
        let view = self.view();
        self.sink.changed(&view);
        view
    }

    /// [`Self::changed`] from a background task: the view may read the
    /// keychain, which blocks.
    async fn send_changed(self: &Arc<Self>) {
        let ai = self.clone();
        if let Ok(view) = tokio::task::spawn_blocking(move || ai.view()).await {
            self.sink.changed(&view);
        }
    }

    fn next_request(&self) -> AiRequestId {
        let mut state = lock(&self.state);
        state.last_request = state.last_request.wrapping_add(1);
        AiRequestId(state.last_request)
    }

    fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        let handle = self.runtime.spawn(task);
        let mut tasks = lock(&self.tasks);
        tasks.retain(|task| !task.is_finished());
        tasks.push(handle);
    }
}

fn account(id: &ProviderId) -> String {
    keychain_account(&id.0)
}

/// The real providers of `gitcroc-ai`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderCalls;

impl AiBackend for ProviderCalls {
    fn detect(&self) -> Vec<DetectedCli> {
        let found = |kind, paths: Vec<PathBuf>| {
            paths
                .into_iter()
                .map(move |path| DetectedCli { kind, path })
        };
        found(ProviderKind::ClaudeCode, ClaudeCodeProvider::detect())
            .chain(found(ProviderKind::Codex, CodexProvider::detect()))
            .collect()
    }

    fn self_test<'a>(&'a self, kind: ProviderKind, path: &'a Path) -> BoxFuture<'a, SelfTest> {
        Box::pin(async move {
            let result = match kind {
                ProviderKind::ClaudeCode => ClaudeCodeProvider::self_test(path).await,
                ProviderKind::Codex => CodexProvider::self_test(path).await,
                ProviderKind::Anthropic | ProviderKind::Openai => {
                    return SelfTest::Failed {
                        reason: format!("{kind} providers run no program"),
                    };
                }
            };
            match result {
                Ok(version) => SelfTest::Passed {
                    version: Some(version),
                },
                Err(SelfTestError::MissingFlags { version, missing }) => {
                    SelfTest::MissingFlags { version, missing }
                }
                Err(SelfTestError::Run(error)) => SelfTest::Failed {
                    reason: error.to_string(),
                },
            }
        })
    }

    fn list_models<'a>(
        &'a self,
        entry: &'a ProviderEntry,
        key: Option<ApiKey>,
    ) -> BoxFuture<'a, ModelsResult> {
        Box::pin(async move {
            // No model is needed to list them.
            let listed = match entry.kind {
                ProviderKind::Anthropic => match key {
                    None => {
                        return ModelsResult::Failed {
                            reason: AiError::NeedsKey.to_string(),
                        };
                    }
                    Some(key) => match AnthropicProvider::new(key, String::new()) {
                        Ok(provider) => provider.list_models().await,
                        Err(error) => Err(error),
                    },
                },
                ProviderKind::Openai => {
                    let base_url = entry.base_url.as_deref().unwrap_or_default();
                    match OpenAiProvider::new(base_url, key, String::new()) {
                        Ok(provider) => provider.list_models().await,
                        Err(error) => Err(error),
                    }
                }
                ProviderKind::ClaudeCode | ProviderKind::Codex => {
                    return ModelsResult::NotOffered;
                }
            };
            match listed {
                Ok(ModelList::Offered(models)) => ModelsResult::Offered { models },
                Ok(ModelList::NotOffered) => ModelsResult::NotOffered,
                Err(error) => ModelsResult::Failed {
                    reason: error.to_string(),
                },
            }
        })
    }
}
