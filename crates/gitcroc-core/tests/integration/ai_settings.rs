//! The `[ai]` table of `settings.toml`: the providers, the default and the
//! AI settings (AI-1, AI-4, AI-7, AI-10, AI-16).

use std::fs;
use std::path::PathBuf;

use gitcroc_core::{
    AiOptions, AiSettings, AiSettingsError, ConfigDir, ConfigWarning, DEFAULT_CONTEXT_BUDGET,
    ProviderDraft, ProviderEntry, ProviderId, ProviderKind, SETTINGS_FILE, SettingsError,
    SettingsStore,
};

struct TempConfig {
    _root: tempfile::TempDir,
    dir: PathBuf,
}

impl TempConfig {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("gitcroc");
        Self { _root: root, dir }
    }

    fn with_file(contents: &str) -> Self {
        let config = Self::new();
        fs::create_dir_all(&config.dir).unwrap();
        fs::write(config.file(), contents).unwrap();
        config
    }

    fn file(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }

    fn text(&self) -> String {
        fs::read_to_string(self.file()).unwrap()
    }
}

impl ConfigDir for TempConfig {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.dir.clone())
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
            draft.path = Some(PathBuf::from("/usr/local/bin/cli"));
        }
        ProviderKind::Mock => {}
    }
    draft
}

/// Saves `draft` with no key handling.
fn save(store: &SettingsStore, draft: ProviderDraft) -> Result<ProviderEntry, SettingsError> {
    store.save_provider(draft, |_, _| Ok::<(), SettingsError>(()))
}

fn id(text: &str) -> ProviderId {
    ProviderId(text.to_owned())
}

#[test]
fn ai_16_ai_on_by_default() {
    // No `[ai]` table.
    let config = TempConfig::with_file("schema = 2\ntheme = \"dark\"\n");
    let store = SettingsStore::load(&config);
    assert!(!store.view().read_only);
    let ai = store.ai();
    assert!(ai.enabled);
    assert_eq!(ai, AiSettings::default());
    assert_eq!(ai.exclude, [".env*", "*.pem", "*.key", "*.p12", "id_rsa*"]);
    assert!(!ai.debug_log);
    assert_eq!(ai.default, None);
    assert!(ai.providers.is_empty());

    // An `[ai]` table without `enabled`.
    let config = TempConfig::with_file("schema = 2\n\n[ai]\ndebug_log = true\n");
    let store = SettingsStore::load(&config);
    assert!(!store.view().read_only);
    let ai = store.ai();
    assert!(ai.enabled);
    assert!(ai.debug_log);
    assert_eq!(ai.exclude, AiSettings::default().exclude);

    // No file at all.
    assert!(SettingsStore::load(&TempConfig::new()).ai().enabled);
}

#[test]
fn ai_16_off_is_saved_and_read_back() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    store
        .set_ai_options(AiOptions {
            enabled: false,
            exclude: vec!["*.secret".to_owned()],
            debug_log: true,
        })
        .unwrap();
    let ai = SettingsStore::load(&config).ai();
    assert!(!ai.enabled);
    assert_eq!(ai.exclude, ["*.secret"]);
    assert!(ai.debug_log);
}

#[test]
fn ai_1_several_providers_one_default() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    let first = save(&store, draft("Work", ProviderKind::Anthropic)).unwrap();
    let second = save(&store, draft("Local", ProviderKind::Openai)).unwrap();
    let third = save(&store, draft("Claude", ProviderKind::ClaudeCode)).unwrap();
    assert_eq!(
        [&first.id, &second.id, &third.id],
        [&id("p1"), &id("p2"), &id("p3")]
    );
    assert_eq!(store.ai().default, None);

    store.set_default_provider(&second.id).unwrap();
    assert_eq!(store.ai().default, Some(id("p2")));
    // A provider that does not exist cannot be the default.
    assert!(matches!(
        store.set_default_provider(&id("p9")),
        Err(SettingsError::Ai(AiSettingsError::UnknownProvider(_)))
    ));

    // The file keeps all three and the default.
    let loaded = SettingsStore::load(&config).ai();
    assert_eq!(loaded.providers.len(), 3);
    assert_eq!(loaded.default, Some(id("p2")));
    assert_eq!(loaded.providers[1].name, "Local");
    assert_eq!(loaded.providers[1].context_budget, DEFAULT_CONTEXT_BUDGET);

    // Deleting the default clears it.
    let deleted = store.delete_provider(&second.id).unwrap();
    assert_eq!(deleted.name, "Local");
    let ai = SettingsStore::load(&config).ai();
    assert_eq!(ai.default, None);
    let names: Vec<&str> = ai
        .providers
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["Work", "Claude"]);

    // A new provider never takes the id of one still listed.
    let fourth = save(&store, draft("Codex", ProviderKind::Codex)).unwrap();
    assert_eq!(fourth.id, id("p4"));
}

#[test]
fn ai_1_saving_an_entry_again_changes_it_in_place() {
    let config = TempConfig::with_file(
        "schema = 2\n\n[ai]\n\n[[ai.provider]]\nid = \"p1\"\nname = \"Work\"\nkind = \"anthropic\"\n\
         model = \"model-a\"\nconsent = \"api.anthropic.com\"\n",
    );
    let store = SettingsStore::load(&config);
    let mut changed = draft("Work, renamed", ProviderKind::Anthropic);
    changed.id = Some(id("p1"));
    changed.model = Some("  model-c ".to_owned());
    changed.context_budget = Some(50_000);
    let entry = save(&store, changed).unwrap();
    assert_eq!(entry.name, "Work, renamed");
    assert_eq!(entry.model.as_deref(), Some("model-c"));
    assert_eq!(entry.context_budget, 50_000);
    // The UI never sends the consent; saving keeps it (AI-6).
    assert_eq!(entry.consent.as_deref(), Some("api.anthropic.com"));
    assert_eq!(SettingsStore::load(&config).ai().providers, [entry]);

    // An id that is not listed is refused, not added.
    let mut stranger = draft("Stranger", ProviderKind::Anthropic);
    stranger.id = Some(id("p7"));
    assert!(matches!(
        save(&store, stranger),
        Err(SettingsError::Ai(AiSettingsError::UnknownProvider(_)))
    ));
}

#[test]
fn ai_1_the_key_step_runs_before_the_file_is_written() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    // A failing key step leaves the file as it was.
    let refused = store.save_provider(draft("Work", ProviderKind::Anthropic), |entry, new| {
        assert_eq!(entry.id, id("p1"));
        assert!(new);
        Err(SettingsError::ReadOnly)
    });
    assert!(refused.is_err());
    assert!(!config.file().exists());
    assert!(store.ai().providers.is_empty());

    let mut seen = None;
    let entry = store
        .save_provider(draft("Work", ProviderKind::Anthropic), |entry, new| {
            seen = Some((entry.id.clone(), new));
            Ok::<(), SettingsError>(())
        })
        .unwrap();
    assert_eq!(seen, Some((entry.id.clone(), true)));
    let mut again = draft("Work", ProviderKind::Anthropic);
    again.id = Some(entry.id.clone());
    store
        .save_provider(again, |_, new| {
            assert!(!new);
            Ok::<(), SettingsError>(())
        })
        .unwrap();
}

#[test]
fn ai_1_validation_refuses_each_bad_field() {
    let store = SettingsStore::load(&TempConfig::new());
    let refused = |change: &dyn Fn(&mut ProviderDraft), kind: ProviderKind| {
        let mut bad = draft("Provider", kind);
        change(&mut bad);
        match save(&store, bad) {
            Err(SettingsError::Ai(error)) => error,
            other => panic!("not refused: {other:?}"),
        }
    };
    use ProviderKind::{Anthropic, ClaudeCode, Codex, Openai};

    assert_eq!(
        refused(&|d| d.name = "  ".to_owned(), Anthropic),
        AiSettingsError::NoName
    );
    assert_eq!(
        refused(&|d| d.model = None, Anthropic),
        AiSettingsError::NoModel(Anthropic)
    );
    assert_eq!(
        refused(&|d| d.model = Some(" ".to_owned()), Openai),
        AiSettingsError::NoModel(Openai)
    );
    assert_eq!(
        refused(&|d| d.base_url = None, Openai),
        AiSettingsError::NoBaseUrl
    );
    for url in [
        "localhost:11434/v1",
        "ftp://example.com",
        "https://",
        "http:// spaced.example.com",
    ] {
        assert_eq!(
            refused(&|d| d.base_url = Some(url.to_owned()), Openai),
            AiSettingsError::BaseUrl,
            "{url}"
        );
    }
    assert_eq!(
        refused(
            &|d| d.base_url = Some("https://api.example.com".to_owned()),
            Anthropic
        ),
        AiSettingsError::NotForKind {
            field: "base_url",
            kind: Anthropic
        }
    );
    assert_eq!(
        refused(&|d| d.path = Some(PathBuf::from("/bin/claude")), Openai),
        AiSettingsError::NotForKind {
            field: "path",
            kind: Openai
        }
    );
    assert_eq!(
        refused(
            &|d| d.base_url = Some("https://api.example.com".to_owned()),
            Codex
        ),
        AiSettingsError::NotForKind {
            field: "base_url",
            kind: Codex
        }
    );
    assert_eq!(
        refused(&|d| d.path = None, ClaudeCode),
        AiSettingsError::NoPath(ClaudeCode)
    );
    assert_eq!(
        refused(&|d| d.path = Some(PathBuf::new()), Codex),
        AiSettingsError::NoPath(Codex)
    );
    for budget in [0, 999, 10_000_001] {
        assert_eq!(
            refused(&|d| d.context_budget = Some(budget), Anthropic),
            AiSettingsError::Budget,
            "{budget}"
        );
    }

    // The limits themselves are allowed, and the CLI kinds need no model.
    for budget in [1_000, 10_000_000] {
        let mut good = draft("Edge", ClaudeCode);
        good.context_budget = Some(budget);
        save(&store, good).unwrap();
    }
    let mut https = draft("Remote", Openai);
    https.base_url = Some("HTTPS://api.example.com/v1/".to_owned());
    save(&store, https).unwrap();
}

#[test]
fn ai_1_a_mock_provider_needs_no_model_and_takes_no_path() {
    let store = SettingsStore::load(&TempConfig::new());
    let saved = save(&store, draft("Mock", ProviderKind::Mock)).unwrap();
    assert_eq!(saved.model, None);
    assert!(!ProviderKind::Mock.takes_key());

    let mut with_path = draft("Mock", ProviderKind::Mock);
    with_path.path = Some(PathBuf::from("/bin/mock"));
    assert!(matches!(
        save(&store, with_path),
        Err(SettingsError::Ai(AiSettingsError::NotForKind {
            field: "path",
            kind: ProviderKind::Mock
        }))
    ));
}

#[test]
fn ai_4_exclude_list_drops_blank_patterns() {
    let store = SettingsStore::load(&TempConfig::new());
    let ai = store
        .set_ai_options(AiOptions {
            enabled: true,
            exclude: vec![" *.pem ".to_owned(), String::new(), "  ".to_owned()],
            debug_log: false,
        })
        .unwrap();
    assert_eq!(ai.exclude, ["*.pem"]);
}

#[test]
fn data_1_ai_table_matches_section_10() {
    let config = TempConfig::new();
    let store = SettingsStore::load(&config);
    save(&store, draft("Work", ProviderKind::Anthropic)).unwrap();
    let mut local = draft("Local", ProviderKind::Openai);
    local.context_budget = Some(32_000);
    save(&store, local).unwrap();
    store.set_default_provider(&id("p2")).unwrap();
    let text = config.text();
    let ai = &text[text.find("\n[ai]\n").unwrap()..];
    assert_eq!(
        ai,
        "\n[ai]\nenabled = true\ndefault = \"p2\"\n\
         exclude = [\".env*\", \"*.pem\", \"*.key\", \"*.p12\", \"id_rsa*\"]\ndebug_log = false\n\n\
         [[ai.provider]]\nid = \"p1\"\nname = \"Work\"\nkind = \"anthropic\"\nmodel = \"model-a\"\n\
         context_budget = 100000\n\n\
         [[ai.provider]]\nid = \"p2\"\nname = \"Local\"\nkind = \"openai\"\nmodel = \"model-b\"\n\
         base_url = \"http://localhost:11434/v1\"\ncontext_budget = 32000\n"
    );
}

#[test]
fn data_1_ai_table_with_unknown_keys_is_read() {
    // As the workspace file's tables: unknown keys are ignored, and a later
    // save leaves them out.
    let config = TempConfig::with_file(
        "schema = 2\n\n[ai]\nfuture = 1\n\n[[ai.provider]]\nid = \"p1\"\nname = \"Work\"\n\
         kind = \"anthropic\"\nmodel = \"model-a\"\ntemperature = 0.5\n",
    );
    let store = SettingsStore::load(&config);
    assert!(!store.view().read_only, "{:?}", store.view().warning);
    let ai = store.ai();
    assert_eq!(ai.providers.len(), 1);
    assert_eq!(ai.providers[0].name, "Work");
    store.set_default_provider(&id("p1")).unwrap();
    let text = config.text();
    assert!(
        !text.contains("future") && !text.contains("temperature"),
        "{text}"
    );
}

#[test]
fn data_1_invalid_ai_table_keeps_the_file_read_only() {
    for (table, reason) in [
        (
            "[[ai.provider]]\nid = \"p1\"\nname = \"Work\"\nkind = \"gemini\"\n",
            "unknown variant",
        ),
        (
            "[[ai.provider]]\nid = \"p1\"\nname = \"Local\"\nkind = \"openai\"\nmodel = \"m\"\n\
             base_url = \"localhost\"\n",
            "base URL",
        ),
        (
            "[[ai.provider]]\nid = \"p1\"\nname = \"A\"\nkind = \"codex\"\npath = \"/bin/codex\"\n\n\
             [[ai.provider]]\nid = \"p1\"\nname = \"B\"\nkind = \"codex\"\npath = \"/bin/codex\"\n",
            "p1",
        ),
        ("[ai]\ndefault = \"p3\"\n", "p3"),
    ] {
        let contents = format!("schema = 2\n\n{table}");
        let config = TempConfig::with_file(&contents);
        let store = SettingsStore::load(&config);
        let view = store.view();
        assert!(view.read_only, "{table}");
        match view.warning {
            Some(ConfigWarning::Unreadable { reason: text, .. }) => {
                assert!(text.contains(reason), "{text}");
            }
            other => panic!("{other:?}"),
        }
        // Nothing is written; the providers stay as the user wrote them.
        assert!(save(&store, draft("New", ProviderKind::Codex)).is_err());
        assert_eq!(config.text(), contents);
    }
}
