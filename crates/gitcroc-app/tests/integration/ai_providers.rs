//! The provider settings behind the IPC (M4-05): keys only in the keychain
//! (SEC-1, D11), the AI features switch (AI-16), models from the provider
//! or the user (AI-1), and the CLI providers' detection and self-test
//! (AI-2). A fake stands in for the providers, so nothing real is called,
//! and every background task is awaited, never timed.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use gitcroc_ai::{ApiKey, MockProvider, Provider, ProviderRequest};
use gitcroc_app::ai::{
    AiBackend, AiError, AiModelsReply, AiProviders, AiRefusal, AiSelfTestReply, AiSink, AiView,
    ConsentRequest, ConsentTarget, DetectedCli, ModelsResult, ProviderView, SelfTest,
};
use gitcroc_app::platform::{Keychain, MemoryKeychain, keychain_account};
use gitcroc_core::{
    AiOptions, AppLog, ConfigDir, LogDir, ProviderDraft, ProviderEntry, ProviderId, ProviderKind,
    SETTINGS_FILE, SettingsStore, WorkspaceId, Workspaces,
};

const KEY: &str = "sk-test-0123456789-SECRET";
const OTHER_KEY: &str = "sk-test-9876543210-OTHER";

/// What the fake answers.
struct Script {
    detected: Vec<DetectedCli>,
    models: ModelsResult,
    self_test: SelfTest,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            detected: Vec::new(),
            models: ModelsResult::NotOffered,
            self_test: SelfTest::Passed {
                version: Some("1.0.0".to_owned()),
            },
        }
    }
}

/// The providers' stand-in: it records each call and answers from a script.
#[derive(Default)]
struct FakeCalls {
    script: Script,
    calls: Mutex<Vec<String>>,
    /// The key each model list was asked with.
    keys: Mutex<Vec<Option<ApiKey>>>,
}

impl FakeCalls {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl AiBackend for FakeCalls {
    fn detect(&self) -> Vec<DetectedCli> {
        self.calls.lock().unwrap().push("detect".to_owned());
        self.script.detected.clone()
    }

    fn self_test<'a>(
        &'a self,
        kind: ProviderKind,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = SelfTest> + Send + 'a>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("self_test {kind:?} {}", path.display()));
        Box::pin(async move { self.script.self_test.clone() })
    }

    fn list_models<'a>(
        &'a self,
        entry: &'a ProviderEntry,
        key: Option<ApiKey>,
    ) -> Pin<Box<dyn Future<Output = ModelsResult> + Send + 'a>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("list_models {:?}", entry.kind));
        self.keys.lock().unwrap().push(key);
        Box::pin(async move { self.script.models.clone() })
    }
}

/// Everything sent toward the UI, as JSON.
#[derive(Default)]
struct Sink {
    payloads: Mutex<Vec<String>>,
    models: Mutex<Vec<AiModelsReply>>,
    self_tests: Mutex<Vec<AiSelfTestReply>>,
}

impl Sink {
    fn record(&self, payload: &impl serde::Serialize) {
        self.payloads
            .lock()
            .unwrap()
            .push(serde_json::to_string(payload).unwrap());
    }
}

impl AiSink for Sink {
    fn changed(&self, view: &AiView) {
        self.record(view);
    }

    fn models(&self, reply: &AiModelsReply) {
        self.record(reply);
        self.models.lock().unwrap().push(reply.clone());
    }

    fn self_test(&self, reply: &AiSelfTestReply) {
        self.record(reply);
        self.self_tests.lock().unwrap().push(reply.clone());
    }
}

struct Dirs(PathBuf);

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("config"))
    }
}

impl LogDir for Dirs {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("log"))
    }
}

struct Fixture {
    root: tempfile::TempDir,
    settings: Arc<SettingsStore>,
    keychain: Arc<MemoryKeychain>,
    calls: Arc<FakeCalls>,
    sink: Arc<Sink>,
    ai: Arc<AiProviders>,
}

impl Fixture {
    fn new(settings: Option<&str>, script: Script) -> Self {
        Self::with_keychain(settings, script, MemoryKeychain::default())
    }

    fn with_keychain(settings: Option<&str>, script: Script, keychain: MemoryKeychain) -> Self {
        let root = tempfile::tempdir().unwrap();
        let dirs = Dirs(root.path().to_owned());
        if let Some(text) = settings {
            fs::create_dir_all(root.path().join("config")).unwrap();
            fs::write(root.path().join("config").join(SETTINGS_FILE), text).unwrap();
        }
        let settings = Arc::new(SettingsStore::load(&dirs));
        let keychain = Arc::new(keychain);
        let calls = Arc::new(FakeCalls {
            script,
            ..FakeCalls::default()
        });
        let sink = Arc::new(Sink::default());
        let ai = AiProviders::new(
            settings.clone(),
            keychain.clone(),
            calls.clone(),
            sink.clone(),
            Arc::new(AppLog::open(&dirs)),
            tokio::runtime::Handle::current(),
        );
        Self {
            root,
            settings,
            keychain,
            calls,
            sink,
            ai,
        }
    }

    /// The workspaces, sharing the settings store as in the app.
    fn workspaces(&self) -> Workspaces {
        Workspaces::load_with(&Dirs(self.root.path().to_owned()), self.settings.clone())
    }

    fn settings_text(&self) -> String {
        fs::read_to_string(self.root.path().join("config").join(SETTINGS_FILE)).unwrap()
    }

    fn log_text(&self) -> String {
        let mut text = String::new();
        if let Ok(entries) = fs::read_dir(self.root.path().join("log")) {
            for entry in entries {
                text.push_str(&fs::read_to_string(entry.unwrap().path()).unwrap());
            }
        }
        text
    }
}

fn draft(name: &str, kind: ProviderKind) -> ProviderDraft {
    let mut draft = ProviderDraft {
        id: None,
        name: name.to_owned(),
        kind,
        model: None,
        base_url: None,
        path: None,
        context_budget: None,
    };
    match kind {
        ProviderKind::Anthropic => draft.model = Some("model-a".to_owned()),
        ProviderKind::Openai => {
            draft.model = Some("model-b".to_owned());
            draft.base_url = Some("http://localhost:11434/v1".to_owned());
        }
        ProviderKind::ClaudeCode | ProviderKind::Codex => {
            draft.path = Some(PathBuf::from("/opt/cli/bin/tool"));
        }
    }
    draft
}

fn id(text: &str) -> ProviderId {
    ProviderId(text.to_owned())
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap()
}

const TWO_PROVIDERS: &str = "schema = 2\n\n[ai]\ndefault = \"p2\"\n\n\
    [[ai.provider]]\nid = \"p1\"\nname = \"Work\"\nkind = \"anthropic\"\nmodel = \"model-a\"\n\
    consent = \"api.anthropic.com\"\n\n\
    [[ai.provider]]\nid = \"p2\"\nname = \"Codex\"\nkind = \"codex\"\npath = \"/opt/codex/bin/codex\"\n\
    consent = \"/opt/codex/bin/codex\"\n";

#[tokio::test(flavor = "multi_thread")]
async fn ai_16_off_runs_no_detection_or_self_test() {
    let off = TWO_PROVIDERS.replace("[ai]\n", "[ai]\nenabled = false\n");
    let fixture = Fixture::new(
        Some(&off),
        Script {
            detected: vec![DetectedCli {
                kind: ProviderKind::ClaudeCode,
                path: PathBuf::from("/usr/bin/claude"),
            }],
            ..Script::default()
        },
    );
    fixture.ai.start();
    fixture.ai.settle().await;
    assert!(
        fixture.calls.calls().is_empty(),
        "{:?}",
        fixture.calls.calls()
    );

    let view = fixture.ai.view();
    assert!(!view.enabled);
    assert!(view.detected.is_empty() && view.offer.is_none());
    assert!(
        view.providers
            .iter()
            .all(|provider| provider.check.is_none())
    );

    // Models and the self-test are refused, and nothing runs.
    for kind in [ProviderKind::Openai, ProviderKind::Codex] {
        assert!(matches!(
            fixture.ai.models(draft("Draft", kind), None),
            Err(AiError::Off)
        ));
        assert!(matches!(
            fixture.ai.self_test(draft("Draft", kind), None),
            Err(AiError::Off)
        ));
    }
    // Saving a CLI provider while off tests nothing either.
    fixture
        .ai
        .save(draft("Claude", ProviderKind::ClaudeCode), None)
        .unwrap();
    fixture.ai.settle().await;
    assert!(
        fixture.calls.calls().is_empty(),
        "{:?}",
        fixture.calls.calls()
    );
    assert!(fixture.sink.models.lock().unwrap().is_empty());
    assert!(fixture.sink.self_tests.lock().unwrap().is_empty());

    // Turning AI on runs them then.
    fixture
        .ai
        .set_options(AiOptions {
            enabled: true,
            exclude: Vec::new(),
            debug_log: false,
        })
        .unwrap();
    fixture.ai.settle().await;
    let mut calls = fixture.calls.calls();
    calls.sort();
    assert_eq!(
        calls,
        [
            "detect",
            "self_test ClaudeCode /opt/cli/bin/tool",
            "self_test Codex /opt/codex/bin/codex",
        ]
    );
    let view = fixture.ai.view();
    assert_eq!(view.offer.unwrap().path, PathBuf::from("/usr/bin/claude"));
    assert!(view.providers[1].check.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_16_turning_off_drops_detection_and_self_tests() {
    let fixture = Fixture::new(Some(TWO_PROVIDERS), Script::default());
    fixture.ai.start();
    fixture.ai.settle().await;
    assert_eq!(
        fixture.ai.view().providers[1].check,
        Some(SelfTest::Passed {
            version: Some("1.0.0".to_owned())
        })
    );
    fixture
        .ai
        .set_options(AiOptions {
            enabled: false,
            exclude: Vec::new(),
            debug_log: false,
        })
        .unwrap();
    let view = fixture.ai.view();
    assert!(
        view.providers
            .iter()
            .all(|provider| provider.check.is_none())
    );
    assert!(view.detected.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_16_off_keeps_providers_keys_and_consent() {
    let fixture = Fixture::new(Some(TWO_PROVIDERS), Script::default());
    fixture.ai.start();
    let mut work = draft("Work", ProviderKind::Anthropic);
    work.id = Some(id("p1"));
    fixture.ai.save(work, Some(KEY.to_owned())).unwrap();
    fixture.ai.settle().await;
    let before = fixture.ai.view();
    let keys = fixture.keychain.entries();

    for enabled in [false, true] {
        fixture
            .ai
            .set_options(AiOptions {
                enabled,
                exclude: before.exclude.clone(),
                debug_log: false,
            })
            .unwrap();
        fixture.ai.settle().await;
        let view = fixture.ai.view();
        assert_eq!(view.enabled, enabled);
        assert_eq!(view.default, Some(id("p2")));
        // The same entries; only the self-test is gone while off.
        let entries = |view: &AiView| -> Vec<ProviderView> {
            view.providers
                .iter()
                .map(|provider| ProviderView {
                    check: None,
                    ..provider.clone()
                })
                .collect()
        };
        assert_eq!(entries(&view), entries(&before));
        assert_eq!(
            view.providers[0].consent.as_deref(),
            Some("api.anthropic.com")
        );
        assert_eq!(
            view.providers[1].consent.as_deref(),
            Some("/opt/codex/bin/codex")
        );
        assert!(view.providers[0].has_key);
        assert_eq!(fixture.keychain.entries(), keys);
    }
    assert_eq!(fixture.ai.view(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_1_keys_only_in_the_keychain() {
    let fixture = Fixture::new(
        None,
        Script {
            self_test: SelfTest::Failed {
                reason: "it exited with code 1".to_owned(),
            },
            ..Script::default()
        },
    );
    fixture.ai.start();
    // Every value an IPC command returns, as the UI gets it.
    let mut payloads = vec![
        json(
            &fixture
                .ai
                .save(draft("Work", ProviderKind::Anthropic), Some(KEY.to_owned()))
                .unwrap(),
        ),
        json(
            &fixture
                .ai
                .save(
                    draft("Local", ProviderKind::Openai),
                    Some(OTHER_KEY.to_owned()),
                )
                .unwrap(),
        ),
        json(
            &fixture
                .ai
                .save(draft("Codex", ProviderKind::Codex), None)
                .unwrap(),
        ),
        json(&fixture.ai.set_default(&id("p1")).unwrap()),
    ];
    // Models and a test of the stored providers use the stored keys.
    let mut stored = draft("Work", ProviderKind::Anthropic);
    stored.id = Some(id("p1"));
    // One at a time, so the keys arrive in order.
    payloads.push(json(&fixture.ai.models(stored.clone(), None).unwrap()));
    fixture.ai.settle().await;
    payloads.push(json(&fixture.ai.self_test(stored, None).unwrap()));
    fixture.ai.settle().await;
    // A key typed for a test is used as typed.
    let mut typed = draft("Local", ProviderKind::Openai);
    typed.id = Some(id("p2"));
    payloads.push(json(
        &fixture
            .ai
            .models(typed, Some("sk-typed".to_owned()))
            .unwrap(),
    ));
    fixture.ai.settle().await;
    payloads.push(json(&fixture.ai.view()));
    // A refused save carries no key in its error.
    let refused = fixture
        .ai
        .save(draft("Codex", ProviderKind::Codex), Some(KEY.to_owned()))
        .unwrap_err();
    payloads.push(refused.to_string());
    payloads.extend(fixture.sink.payloads.lock().unwrap().iter().cloned());

    assert_eq!(
        *fixture.calls.keys.lock().unwrap(),
        [
            Some(ApiKey::new(KEY)),
            Some(ApiKey::new(KEY)),
            Some(ApiKey::new("sk-typed"))
        ]
    );
    let view = fixture.ai.view();
    assert!(view.providers[0].has_key && view.providers[1].has_key);
    assert!(!view.providers[2].has_key);
    assert_eq!(
        fixture.keychain.entries(),
        [
            (keychain_account("p1"), KEY.to_owned()),
            (keychain_account("p2"), OTHER_KEY.to_owned()),
        ]
        .into_iter()
        .collect()
    );

    let settings = fixture.settings_text();
    let log = fixture.log_text();
    // The failed self-test was logged, so the log is not empty.
    assert!(log.contains("it exited with code 1"), "{log}");
    assert!(payloads.len() > 10, "{payloads:?}");
    for secret in [KEY, OTHER_KEY, "sk-typed"] {
        assert!(!settings.contains(secret), "{settings}");
        assert!(!log.contains(secret), "{log}");
        for payload in &payloads {
            assert!(!payload.contains(secret), "{payload}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_1_unreachable_keychain_is_reported_never_a_file() {
    let fixture = Fixture::with_keychain(None, Script::default(), MemoryKeychain::unreachable());
    let error = fixture
        .ai
        .save(draft("Work", ProviderKind::Anthropic), Some(KEY.to_owned()))
        .unwrap_err();
    assert!(matches!(error, AiError::Keychain(_)), "{error:?}");
    assert!(!error.to_string().contains(KEY));
    // Nothing was saved, so there is no file to hold the key either.
    assert!(fixture.ai.view().providers.is_empty());
    assert!(
        !fixture
            .root
            .path()
            .join("config")
            .join(SETTINGS_FILE)
            .exists()
    );

    // A provider that needs no key is still saved, and the keychain's state
    // is reported in the provider settings.
    fixture
        .ai
        .save(draft("Codex", ProviderKind::Codex), None)
        .unwrap();
    let view = fixture.ai.view();
    assert_eq!(view.providers.len(), 1);
    assert!(view.keychain_error.is_some());
    assert!(!fixture.settings_text().contains(KEY));
}

#[tokio::test(flavor = "multi_thread")]
async fn d11_key_removed_with_its_provider() {
    let fixture = Fixture::new(None, Script::default());
    fixture
        .ai
        .save(draft("Work", ProviderKind::Anthropic), Some(KEY.to_owned()))
        .unwrap();
    fixture
        .ai
        .save(
            draft("Local", ProviderKind::Openai),
            Some(OTHER_KEY.to_owned()),
        )
        .unwrap();
    assert_eq!(fixture.keychain.entries().len(), 2);

    let view = fixture.ai.delete(&id("p2")).unwrap();
    assert_eq!(view.providers.len(), 1);
    assert_eq!(
        fixture.keychain.entries(),
        [(keychain_account("p1"), KEY.to_owned())]
            .into_iter()
            .collect()
    );

    // A key left behind under a reused id never reaches the new provider.
    fixture
        .keychain
        .write(&keychain_account("p2"), "left behind")
        .unwrap();
    fixture
        .ai
        .save(draft("Local again", ProviderKind::Openai), None)
        .unwrap();
    assert!(!fixture.ai.view().providers[1].has_key);
    assert_eq!(fixture.keychain.entries().len(), 1);

    // An empty key removes it; no key keeps it.
    let mut local = draft("Local again", ProviderKind::Openai);
    local.id = Some(id("p2"));
    fixture
        .ai
        .save(local.clone(), Some(OTHER_KEY.to_owned()))
        .unwrap();
    fixture.ai.save(local.clone(), None).unwrap();
    assert!(fixture.ai.view().providers[1].has_key);
    fixture.ai.save(local, Some(String::new())).unwrap();
    assert!(!fixture.ai.view().providers[1].has_key);
    assert!(
        !fixture
            .keychain
            .entries()
            .contains_key(&keychain_account("p2"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_1_key_rules_per_kind() {
    let fixture = Fixture::new(None, Script::default());
    // The Anthropic API takes no request without a key.
    assert!(matches!(
        fixture
            .ai
            .save(draft("Work", ProviderKind::Anthropic), None),
        Err(AiError::NeedsKey)
    ));
    assert!(matches!(
        fixture
            .ai
            .save(draft("Work", ProviderKind::Anthropic), Some(String::new())),
        Err(AiError::NeedsKey)
    ));
    // A CLI provider uses its own login and takes no key.
    assert!(matches!(
        fixture
            .ai
            .save(draft("Codex", ProviderKind::Codex), Some(KEY.to_owned())),
        Err(AiError::TakesNoKey(ProviderKind::Codex))
    ));
    // An OpenAI-compatible endpoint may need none, such as a local model.
    fixture
        .ai
        .save(draft("Local", ProviderKind::Openai), None)
        .unwrap();
    assert!(fixture.keychain.entries().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_1_model_names_come_from_the_provider_or_the_user() {
    // A provider with a list: the names in its order, nothing added.
    let offered = vec!["zeta-2".to_owned(), "alpha-1".to_owned()];
    let fixture = Fixture::new(
        None,
        Script {
            models: ModelsResult::Offered {
                models: offered.clone(),
            },
            ..Script::default()
        },
    );
    let mut new = draft("New", ProviderKind::Openai);
    new.model = None;
    let request = fixture.ai.models(new, None).unwrap();
    fixture.ai.settle().await;
    assert_eq!(
        *fixture.sink.models.lock().unwrap(),
        [AiModelsReply {
            request_id: request,
            result: ModelsResult::Offered { models: offered },
        }]
    );

    // A provider without one: the typed name is kept as typed.
    let fixture = Fixture::new(None, Script::default());
    let mut local = draft("Local", ProviderKind::Openai);
    local.model = Some("my-own-model:7b".to_owned());
    let request = fixture.ai.models(local.clone(), None).unwrap();
    fixture.ai.settle().await;
    assert_eq!(
        *fixture.sink.models.lock().unwrap(),
        [AiModelsReply {
            request_id: request,
            result: ModelsResult::NotOffered,
        }]
    );
    fixture.ai.save(local, None).unwrap();
    assert_eq!(
        fixture.ai.view().providers[0].model.as_deref(),
        Some("my-own-model:7b")
    );
    assert!(
        fixture
            .settings_text()
            .contains("model = \"my-own-model:7b\"")
    );

    // A draft that cannot be reached is refused before anything runs.
    let mut nowhere = draft("Nowhere", ProviderKind::Openai);
    nowhere.base_url = Some("localhost".to_owned());
    assert!(matches!(
        fixture.ai.models(nowhere, None),
        Err(AiError::Invalid(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_2_cli_providers_are_self_tested_at_start() {
    let fixture = Fixture::new(
        Some(TWO_PROVIDERS),
        Script {
            self_test: SelfTest::MissingFlags {
                version: "codex-cli 0.1.0".to_owned(),
                missing: vec!["--ephemeral".to_owned()],
            },
            ..Script::default()
        },
    );
    fixture.ai.start();
    fixture.ai.settle().await;
    let view = fixture.ai.view();
    // The API provider has no self-test; the CLI one carries the reason.
    assert_eq!(view.providers[0].check, None);
    assert_eq!(
        view.providers[1].check,
        Some(SelfTest::MissingFlags {
            version: "codex-cli 0.1.0".to_owned(),
            missing: vec!["--ephemeral".to_owned()],
        })
    );
    // The change reached the UI as an event.
    let last = fixture
        .sink
        .payloads
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert_eq!(last, json(&view));

    // Saving a CLI provider tests it again; Test reports to the caller.
    let mut codex = draft("Codex", ProviderKind::Codex);
    codex.id = Some(id("p2"));
    fixture.ai.save(codex.clone(), None).unwrap();
    let request = fixture.ai.self_test(codex, None).unwrap();
    fixture.ai.settle().await;
    assert_eq!(
        fixture
            .calls
            .calls()
            .iter()
            .filter(|call| call.starts_with("self_test"))
            .count(),
        3
    );
    assert_eq!(
        fixture.sink.self_tests.lock().unwrap()[0].request_id,
        request
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_2_api_self_test_lists_the_models() {
    let fixture = Fixture::new(
        None,
        Script {
            models: ModelsResult::Failed {
                reason: "the provider answered with HTTP 401".to_owned(),
            },
            ..Script::default()
        },
    );
    let request = fixture
        .ai
        .self_test(draft("Local", ProviderKind::Openai), None)
        .unwrap();
    fixture.ai.settle().await;
    assert_eq!(fixture.calls.calls(), ["list_models Openai"]);
    assert_eq!(
        *fixture.sink.self_tests.lock().unwrap(),
        [AiSelfTestReply {
            request_id: request,
            result: SelfTest::Failed {
                reason: "the provider answered with HTTP 401".to_owned()
            },
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_2_detected_cli_is_offered_never_added() {
    let claude = DetectedCli {
        kind: ProviderKind::ClaudeCode,
        path: PathBuf::from("/opt/cli/bin/tool"),
    };
    let codex = DetectedCli {
        kind: ProviderKind::Codex,
        path: PathBuf::from("/usr/bin/codex"),
    };
    let fixture = Fixture::new(
        None,
        Script {
            detected: vec![claude.clone(), codex.clone()],
            ..Script::default()
        },
    );
    fixture.ai.start();
    fixture.ai.settle().await;
    let view = fixture.ai.view();
    assert_eq!(view.detected, [claude.clone(), codex.clone()]);
    assert_eq!(view.offer, Some(claude));
    assert!(view.providers.is_empty());
    assert!(
        !fixture
            .root
            .path()
            .join("config")
            .join(SETTINGS_FILE)
            .exists()
    );

    // Once the first is configured, the next is offered.
    fixture
        .ai
        .save(draft("Claude", ProviderKind::ClaudeCode), None)
        .unwrap();
    assert_eq!(fixture.ai.view().offer, Some(codex));
}

/// Model names that would show a hardcoded model: the families' prefixes
/// as they start a string literal.
const MODEL_PREFIXES: [&str; 14] = [
    "claude-", "gpt-", "o1-", "o3-", "o4-", "gemini", "llama", "mistral", "qwen", "sonnet", "opus",
    "haiku", "deepseek", "codex-",
];

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if !["node_modules", "tests", "test", "fixtures"].contains(&name.as_str()) {
                sources(&path, found);
            }
        } else if (name.ends_with(".rs") || name.ends_with(".ts") || name.ends_with(".tsx"))
            && !name.contains(".test.")
        {
            found.push(path);
        }
    }
}

/// The string literals of `code` that start like a model name.
fn model_literals(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    for quote in ['"', '\'', '`'] {
        for literal in code.split(quote).skip(1).step_by(2) {
            let lower = literal.to_ascii_lowercase();
            if MODEL_PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(prefix))
            {
                found.push(literal.to_owned());
            }
        }
    }
    found
}

#[test]
fn ai_1_no_model_name_is_hardcoded() {
    // The scan sees a model name where there is one, and only there.
    assert_eq!(
        model_literals("let m = \"Claude-Sonnet-9\"; let n = 'gpt-9o'; let id = 'o1';"),
        ["Claude-Sonnet-9", "gpt-9o"]
    );
    assert!(model_literals("const PROGRAM: &str = \"Claude Code\";").is_empty());

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in ["crates", "ui/src"] {
        sources(&root.join(dir), &mut files);
    }
    assert!(files.len() > 50, "{files:?}");
    let mut found = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        // A Rust file's unit tests sit at its end.
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for literal in model_literals(code) {
            found.push(format!("{}: {literal}", file.display()));
        }
    }
    assert!(
        found.is_empty(),
        "AI-1: model names in the code: {found:#?}"
    );
}

/// What M4-08's job does with a provider: the checks, then the request.
async fn ask(
    fixture: &Fixture,
    workspaces: &Workspaces,
    workspace: &WorkspaceId,
    mock: &MockProvider,
) -> Result<(), AiRefusal> {
    let chosen = fixture.ai.choose(workspaces, workspace)?;
    fixture.ai.ready(chosen)?;
    let request = ProviderRequest {
        system: String::new(),
        user: "a conflict".to_owned(),
        follow_up: Vec::new(),
        temperature: Some(0.0),
    };
    mock.complete(&request).await.unwrap();
    Ok(())
}

/// The consent the provider still needs, from its refusal.
fn consent_needed(result: Result<(), AiRefusal>) -> ConsentRequest {
    match result {
        Err(AiRefusal::ConsentNeeded(request)) => request,
        other => panic!("expected consent_needed, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_6_no_request_without_consent() {
    let fixture = Fixture::new(None, Script::default());
    let workspaces = fixture.workspaces();
    let workspace = workspaces.active().unwrap();
    let mock = MockProvider::texts(["{\"hunks\": []}"]);
    fixture
        .ai
        .save(draft("Local", ProviderKind::Openai), None)
        .unwrap();
    fixture.ai.set_default(&id("p1")).unwrap();

    // The core asks first and says what is sent and where.
    let request = consent_needed(ask(&fixture, &workspaces, &workspace, &mock).await);
    assert!(mock.requests().is_empty());
    assert_eq!(request.provider_id, id("p1"));
    assert_eq!(request.provider_name, "Local");
    assert_eq!(
        request.to,
        ConsentTarget::Host {
            host: "localhost:11434".to_owned()
        }
    );
    let sent = request.sent.join("\n");
    for part in [
        "path",
        "operation",
        "labels",
        "40 lines",
        "10 commits",
        "2,000 characters",
    ] {
        assert!(sent.contains(part), "{part} missing from {sent}");
    }
    // The view offers the same request, for the dialog (AI-6).
    let view = fixture.ai.view();
    assert_eq!(view.providers[0].consent_needed.as_ref(), Some(&request));

    // Consent to another host than the one shown is refused.
    assert!(matches!(
        fixture.ai.consent(&id("p1"), "api.example.com"),
        Err(AiError::ConsentChanged { .. })
    ));
    assert!(mock.requests().is_empty());

    let view = fixture.ai.consent(&id("p1"), "localhost:11434").unwrap();
    assert_eq!(
        view.providers[0].consent.as_deref(),
        Some("localhost:11434")
    );
    assert_eq!(view.providers[0].consent_needed, None);
    assert!(
        fixture
            .settings_text()
            .contains("consent = \"localhost:11434\"")
    );
    ask(&fixture, &workspaces, &workspace, &mock).await.unwrap();
    assert_eq!(mock.requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_6_consent_asked_again_when_the_host_or_program_changes() {
    let fixture = Fixture::new(None, Script::default());
    let workspaces = fixture.workspaces();
    let workspace = workspaces.active().unwrap();
    let mock = MockProvider::texts(["{}", "{}", "{}"]);
    fixture
        .ai
        .save(draft("Local", ProviderKind::Openai), None)
        .unwrap();
    fixture.ai.set_default(&id("p1")).unwrap();
    fixture.ai.consent(&id("p1"), "localhost:11434").unwrap();
    ask(&fixture, &workspaces, &workspace, &mock).await.unwrap();

    // Another path on the same host is the same place.
    let mut same_host = draft("Local", ProviderKind::Openai);
    same_host.id = Some(id("p1"));
    same_host.base_url = Some("http://LOCALHOST:11434/api/v1/".to_owned());
    fixture.ai.save(same_host.clone(), None).unwrap();
    ask(&fixture, &workspaces, &workspace, &mock).await.unwrap();

    let mut moved = same_host;
    moved.base_url = Some("https://user@openrouter.example.com/api/v1".to_owned());
    fixture.ai.save(moved, None).unwrap();
    let request = consent_needed(ask(&fixture, &workspaces, &workspace, &mock).await);
    assert_eq!(
        request.to,
        ConsentTarget::Host {
            host: "openrouter.example.com".to_owned()
        }
    );
    assert_eq!(mock.requests().len(), 2);

    // A CLI provider names its program, and a new program asks again.
    let mut cli = draft("Codex", ProviderKind::Codex);
    fixture.ai.save(cli.clone(), None).unwrap();
    fixture.ai.set_default(&id("p2")).unwrap();
    let request = consent_needed(ask(&fixture, &workspaces, &workspace, &mock).await);
    let program = PathBuf::from("/opt/cli/bin/tool").display().to_string();
    assert_eq!(
        request.to,
        ConsentTarget::Program {
            path: program.clone()
        }
    );
    fixture.ai.consent(&id("p2"), &program).unwrap();
    ask(&fixture, &workspaces, &workspace, &mock).await.unwrap();
    cli.id = Some(id("p2"));
    cli.path = Some(PathBuf::from("/usr/local/bin/codex"));
    fixture.ai.save(cli, None).unwrap();
    consent_needed(ask(&fixture, &workspaces, &workspace, &mock).await);
    assert_eq!(mock.requests().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_6_workspace_switch_disables_ai() {
    let fixture = Fixture::new(None, Script::default());
    let workspaces = fixture.workspaces();
    let off = workspaces.active().unwrap();
    let on = workspaces.create("Other").unwrap();
    let mock = MockProvider::texts(["{}", "{}"]);
    fixture
        .ai
        .save(draft("Local", ProviderKind::Openai), None)
        .unwrap();
    fixture.ai.set_default(&id("p1")).unwrap();
    fixture.ai.consent(&id("p1"), "localhost:11434").unwrap();

    assert!(!workspaces.set_ai(&off, false).unwrap().ai);
    assert!(matches!(
        ask(&fixture, &workspaces, &off, &mock).await,
        Err(AiRefusal::Disabled)
    ));
    assert!(mock.requests().is_empty());
    // Only that workspace.
    ask(&fixture, &workspaces, &on, &mock).await.unwrap();
    assert_eq!(mock.requests().len(), 1);

    // AI features off is refused first (AI-16).
    let options = |enabled| AiOptions {
        enabled,
        exclude: Vec::new(),
        debug_log: false,
    };
    fixture.ai.set_options(options(false)).unwrap();
    assert!(matches!(
        ask(&fixture, &workspaces, &off, &mock).await,
        Err(AiRefusal::Off)
    ));
    fixture.ai.set_options(options(true)).unwrap();

    workspaces.set_ai(&off, true).unwrap();
    ask(&fixture, &workspaces, &off, &mock).await.unwrap();
    assert_eq!(mock.requests().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_6_refusals_before_consent() {
    let fixture = Fixture::new(
        None,
        Script {
            self_test: SelfTest::Failed {
                reason: "not found".to_owned(),
            },
            ..Script::default()
        },
    );
    let workspaces = fixture.workspaces();
    let workspace = workspaces.active().unwrap();
    let mock = MockProvider::default();

    // No provider: M4-09 opens the provider settings (AI-3).
    fixture
        .ai
        .save(draft("Codex", ProviderKind::Codex), None)
        .unwrap();
    assert!(matches!(
        ask(&fixture, &workspaces, &workspace, &mock).await,
        Err(AiRefusal::NoProvider)
    ));
    // A CLI provider whose self-test failed says why.
    fixture.ai.set_default(&id("p1")).unwrap();
    fixture.ai.settle().await;
    match ask(&fixture, &workspaces, &workspace, &mock).await {
        Err(AiRefusal::Unusable { reason }) => assert_eq!(reason, "not found"),
        other => panic!("expected unusable, got {other:?}"),
    }
    assert!(mock.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ai_16_off_refuses_consent() {
    let fixture = Fixture::new(None, Script::default());
    fixture
        .ai
        .save(draft("Local", ProviderKind::Openai), None)
        .unwrap();
    fixture
        .ai
        .set_options(AiOptions {
            enabled: false,
            exclude: Vec::new(),
            debug_log: false,
        })
        .unwrap();
    assert!(matches!(
        fixture.ai.consent(&id("p1"), "localhost:11434"),
        Err(AiError::Off)
    ));
    assert_eq!(fixture.ai.view().providers[0].consent, None);
}
