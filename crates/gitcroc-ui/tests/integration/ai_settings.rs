//! M6-23a: the Settings dialog's AI tab (AI-1, AI-2, AI-4, AI-7, AI-16,
//! SEC-1, SEC-3, UI-22), the port of `ui/src/ai/ai.test.tsx` and
//! `ui/e2e/ai.spec.ts`. The providers are the fixture's scripted fake, with
//! the in-memory keychain: no test reaches a network or a real CLI, and
//! nothing waits on time; held answers are released by the test.

use std::path::PathBuf;

use gitcroc_app::ai::{AiView, DetectedCli, ModelsResult, SelfTest};
use gitcroc_core::{AiOptions, ProviderDraft, ProviderKind, WorkspaceId};
use gitcroc_ui::settings_dialog::{SettingsDialog, SettingsTab};
use gitcroc_ui::strings;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{ElementId, Entity, Role, TestAppContext, VisualTestContext};

use crate::history::{click, label_of, labels_in, press, tab, wait_for_element, wait_until};
use crate::support::{AiBackend, AiScript, Fixture, Options, main_window};

const KEY: &str = "sk-test-0123456789-SECRET";
const OLDER_KEY: &str = "an older key";

fn start(script: AiScript) -> Fixture {
    Fixture::start_with(Options {
        ai: AiBackend::Script(script),
        ..Options::default()
    })
}

fn named(id: &str, at: usize) -> ElementId {
    ElementId::NamedInteger(id.to_owned().into(), at as u64)
}

fn secondary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

/// Replaces the text of field `id` with `text`.
fn type_into(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    click(cx, id.into());
    press(cx, &secondary("a"));
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

fn value(cx: &mut VisualTestContext, id: &'static str) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.value().map(str::to_owned))
    })
}

fn checked(cx: &mut VisualTestContext, id: &'static str) -> Option<bool> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.checked())
    })
}

fn selected(cx: &mut VisualTestContext, id: ElementId) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .is_some_and(|found| found.selected() == Some(true) || found.checked() == Some(true))
    })
}

fn disabled(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    crate::stage::disabled(cx, id.into())
}

fn focused(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    crate::stage::focused(cx, id.into())
}

/// The labels of the numbered elements `id`, in order.
fn numbered_labels(cx: &mut VisualTestContext, id: &str) -> Vec<String> {
    (0..).map_while(|at| label_of(cx, named(id, at))).collect()
}

/// Every label, value and description the window draws.
fn rendered_text(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .elements()
            .iter()
            .flat_map(|found| [found.label(), found.value(), found.description()])
            .flatten()
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Opens Settings from the toolbar, at its AI tab.
fn open_settings(cx: &mut VisualTestContext) {
    wait_for_element(cx, "toolbar-settings".into());
    assert!(!disabled(cx, "toolbar-settings"));
    click(cx, "toolbar-settings".into());
    wait_for_element(cx, "settings-dialog".into());
    wait_for_element(cx, "ai-features".into());
}

/// The exclude list's text, while it is shown.
fn exclude_value(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("ai-exclude")?;
        window
            .within("ai-exclude")
            .try_find_by_label(strings::ai::EXCLUDE)
            .map(|found| found.value().unwrap_or_default().to_owned())
    })
}

/// The provider list's lines.
fn providers(cx: &mut VisualTestContext) -> Vec<String> {
    labels_in(cx, "ai-providers".into(), Role::ListItem)
}

fn ai_view(f: &Fixture) -> AiView {
    let app = f.app().clone();
    f.runtime
        .block_on(async move { app.ai_providers_get().await })
        .unwrap()
}

fn draft(name: &str, kind: ProviderKind) -> ProviderDraft {
    ProviderDraft {
        id: None,
        name: name.to_owned(),
        kind,
        model: Some("small-model".to_owned()),
        base_url: (kind == ProviderKind::Openai).then(|| "https://api.example.com/v1".to_owned()),
        path: None,
        context_budget: None,
    }
}

/// Saves a provider through the core, as an earlier run would have.
fn save(f: &Fixture, draft: ProviderDraft, key: Option<&str>) -> AiView {
    let app = f.app().clone();
    let key = key.map(str::to_owned);
    f.runtime
        .block_on(async move { app.ai_provider_save(draft, key).await })
        .unwrap()
}

fn set_options(f: &Fixture, enabled: bool) {
    let app = f.app().clone();
    let view = ai_view(f);
    f.runtime
        .block_on(async move {
            app.ai_settings_set(AiOptions {
                enabled,
                exclude: view.exclude,
                debug_log: view.debug_log,
            })
            .await
        })
        .unwrap();
}

fn offered() -> ModelsResult {
    ModelsResult::Offered {
        models: vec!["small-model".to_owned(), "large-model".to_owned()],
    }
}

fn settings_dialog(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
) -> Option<Entity<SettingsDialog>> {
    cx.update(|_, cx| window.read(cx).dialog_of::<SettingsDialog>())
}

#[gpui::test]
fn ai_1_the_kind_list_is_the_cores(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    // Which gitcroc is running shows below the tabs.
    let about = label_of(&mut cx, "settings-about".into()).unwrap();
    assert!(about.starts_with("gitcroc version "), "{about}");
    assert_eq!(
        labels_in(&mut cx, "settings-tabs".into(), Role::Tab),
        [strings::settings::TAB_AI]
    );

    click(&mut cx, "ai-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    assert_eq!(
        label_of(&mut cx, "provider-form".into()).as_deref(),
        Some(strings::ai::NEW_PROVIDER)
    );
    assert_eq!(
        numbered_labels(&mut cx, "provider-kind"),
        ["Anthropic", "OpenAI-compatible", "Claude Code", "Codex"]
    );
    drop(cx);
    drop(f);
}

#[gpui::test]
fn ai_1_the_mock_kind_shows_only_when_the_core_offers_it(cx: &mut TestAppContext) {
    let f = start(AiScript {
        mock: true,
        ..AiScript::default()
    });
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    click(&mut cx, "ai-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    assert_eq!(
        numbered_labels(&mut cx, "provider-kind"),
        [
            "Anthropic",
            "OpenAI-compatible",
            "Claude Code",
            "Codex",
            "Mock"
        ]
    );

    // The mock takes no key, base URL or program, and its model is optional.
    click(&mut cx, named("provider-kind", 4));
    assert!(selected(&mut cx, named("provider-kind", 4)));
    assert!(!shown(&mut cx, "provider-key"));
    assert!(!shown(&mut cx, "provider-base-url"));
    assert!(!shown(&mut cx, "provider-path"));
    assert_eq!(
        label_of(&mut cx, "provider-model-field".into()).as_deref(),
        Some(strings::ai::MODEL_OPTIONAL)
    );
}

/// The port of `ui/e2e/ai.spec.ts`.
#[gpui::test]
fn ai_1_add_a_provider_pick_a_fetched_model_and_keep_it_through_ai_off_and_on(
    cx: &mut TestAppContext,
) {
    let f = start(AiScript {
        models: offered(),
        ..AiScript::default()
    });
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    assert_eq!(
        label_of(&mut cx, "ai-no-provider".into()).as_deref(),
        Some(strings::ai::NO_PROVIDER)
    );

    click(&mut cx, "ai-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    click(&mut cx, named("provider-kind", 1));
    type_into(&mut cx, "provider-name", "Local");
    type_into(&mut cx, "provider-base-url", "http://localhost:11434/v1");
    click(&mut cx, "provider-fetch-models".into());
    wait_for_element(&mut cx, named("provider-model", 1));
    // No model is preselected.
    assert_eq!(
        numbered_labels(&mut cx, "provider-model"),
        ["small-model", "large-model"]
    );
    assert!(!selected(&mut cx, named("provider-model", 0)));
    assert!(!selected(&mut cx, named("provider-model", 1)));
    click(&mut cx, named("provider-model", 1));
    assert!(selected(&mut cx, named("provider-model", 1)));
    click(&mut cx, "provider-save".into());
    wait_until(&mut cx, |cx| !shown(cx, "provider-form"));

    wait_until(&mut cx, |cx| {
        providers(cx) == ["Local · OpenAI-compatible · large-model"]
    });
    click(&mut cx, named("ai-provider-default", 0));
    let line = "Local (default) · OpenAI-compatible · large-model";
    wait_until(&mut cx, |cx| providers(cx) == [line]);
    assert!(disabled(&mut cx, named("ai-provider-default", 0)));

    click(&mut cx, "ai-features".into());
    wait_until(&mut cx, |cx| checked(cx, "ai-features") == Some(false));
    assert!(!shown(&mut cx, "ai-providers"));
    assert_eq!(
        labels_in(&mut cx, "ai-section".into(), Role::Button),
        Vec::<String>::new()
    );
    assert!(!ai_view(&f).enabled);

    click(&mut cx, "ai-features".into());
    wait_until(&mut cx, |cx| providers(cx) == [line]);
    let view = ai_view(&f);
    assert!(view.enabled);
    assert_eq!(view.default, Some(view.providers[0].id.clone()));
    assert_eq!(view.providers[0].model.as_deref(), Some("large-model"));
}

#[gpui::test]
fn ai_1_no_model_is_preselected(cx: &mut TestAppContext) {
    let f = start(AiScript {
        models: offered(),
        ..AiScript::default()
    });
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    click(&mut cx, "ai-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    click(&mut cx, named("provider-kind", 1));
    type_into(&mut cx, "provider-name", "Local");
    type_into(&mut cx, "provider-base-url", "http://localhost:11434/v1");
    click(&mut cx, "provider-fetch-models".into());
    wait_for_element(&mut cx, named("provider-model", 0));
    assert!(!selected(&mut cx, named("provider-model", 0)));

    // Saved without a model, the core refuses, the form says why and
    // nothing is saved.
    click(&mut cx, "provider-save".into());
    wait_for_element(&mut cx, "provider-error".into());
    assert!(shown(&mut cx, "provider-form"));
    assert!(ai_view(&f).providers.is_empty());
}

#[gpui::test]
fn ai_1_a_provider_offering_no_list_gets_a_text_field(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    click(&mut cx, "ai-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    click(&mut cx, named("provider-kind", 1));
    type_into(&mut cx, "provider-name", "Local");
    type_into(&mut cx, "provider-base-url", "http://localhost:11434/v1");
    click(&mut cx, "provider-fetch-models".into());
    wait_for_element(&mut cx, "provider-models-none".into());
    assert!(!shown(&mut cx, named("provider-model", 0)));
    type_into(&mut cx, "provider-model-field", "typed-model");
    click(&mut cx, "provider-save".into());
    wait_until(&mut cx, |cx| !shown(cx, "provider-form"));
    let view = ai_view(&f);
    assert_eq!(view.providers[0].model.as_deref(), Some("typed-model"));
}

/// While a model list or a test is asked for, its button is busy and a
/// second press does nothing.
#[gpui::test]
fn ai_1_fetch_models_and_test_are_busy_while_asked(cx: &mut TestAppContext) {
    let f = start(AiScript {
        models: offered(),
        hold_answers: true,
        ..AiScript::default()
    });
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    click(&mut cx, "ai-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    click(&mut cx, named("provider-kind", 1));
    type_into(&mut cx, "provider-name", "Local");
    type_into(&mut cx, "provider-base-url", "http://localhost:11434/v1");

    click(&mut cx, "provider-fetch-models".into());
    wait_for_element(&mut cx, "provider-models-fetching".into());
    assert!(disabled(&mut cx, "provider-fetch-models"));
    assert!(disabled(&mut cx, "provider-test"));
    click(&mut cx, "provider-fetch-models".into());
    f.release_ai_answer();
    wait_for_element(&mut cx, named("provider-model", 0));
    assert!(!disabled(&mut cx, "provider-fetch-models"));

    click(&mut cx, "provider-test".into());
    wait_until(&mut cx, |cx| {
        label_of(cx, "provider-test-result".into()).as_deref()
            == Some(strings::ai::test_result(strings::ai::TESTING).as_str())
    });
    assert!(disabled(&mut cx, "provider-test"));
    f.release_ai_answer();
    let passed = strings::ai::test_result(strings::ai::WORKS);
    wait_until(&mut cx, |cx| {
        label_of(cx, "provider-test-result".into()).as_deref() == Some(passed.as_str())
    });
    assert!(!disabled(&mut cx, "provider-test"));
}

#[gpui::test]
fn ai_2_cli_providers_show_detected_paths_and_the_self_test(cx: &mut TestAppContext) {
    let claude = PathBuf::from("/opt/claude/bin/claude");
    let other = PathBuf::from("/usr/local/bin/claude");
    let f = start(AiScript {
        detected: vec![
            DetectedCli {
                kind: ProviderKind::ClaudeCode,
                path: claude.clone(),
            },
            DetectedCli {
                kind: ProviderKind::ClaudeCode,
                path: other.clone(),
            },
        ],
        self_test: SelfTest::MissingFlags {
            version: "2.0.1".to_owned(),
            missing: vec!["--tools".to_owned()],
        },
        ..AiScript::default()
    });
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    let shown_path = claude.display().to_string();
    wait_until(&mut cx, |cx| {
        label_of(cx, "ai-offer".into()) == Some(strings::ai::found("Claude Code", &shown_path))
    });

    click(&mut cx, "ai-offer-add".into());
    wait_for_element(&mut cx, "provider-form".into());
    assert!(selected(&mut cx, named("provider-kind", 2)));
    assert_eq!(
        value(&mut cx, "provider-path").as_deref(),
        Some(shown_path.as_str())
    );
    assert_eq!(
        numbered_labels(&mut cx, "provider-detected-path"),
        [shown_path.clone(), other.display().to_string()]
    );
    assert!(selected(&mut cx, named("provider-detected-path", 0)));
    // The CLI kinds take no key and offer no model list.
    assert!(!shown(&mut cx, "provider-key"));
    assert!(!shown(&mut cx, "provider-fetch-models"));

    click(&mut cx, named("provider-detected-path", 1));
    assert_eq!(
        value(&mut cx, "provider-path"),
        Some(other.display().to_string())
    );
    click(&mut cx, named("provider-detected-path", 0));

    // The missing flag is named.
    click(&mut cx, "provider-test".into());
    let lacks = strings::ai::test_result(&strings::ai::lacks("2.0.1", "--tools"));
    wait_until(&mut cx, |cx| {
        label_of(cx, "provider-test-result".into()).as_deref() == Some(lacks.as_str())
    });

    click(&mut cx, "provider-save".into());
    wait_until(&mut cx, |cx| {
        providers(cx)
            .first()
            .is_some_and(|line| line.ends_with("2.0.1 lacks --tools"))
    });
    let view = ai_view(&f);
    assert_eq!(view.providers[0].path.as_ref(), Some(&claude));
}

#[gpui::test]
fn ai_16_off_hides_the_ai_section_except_the_switch(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    save(&f, draft("Hosted", ProviderKind::Openai), None);
    set_options(&f, false);
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    assert_eq!(checked(&mut cx, "ai-features"), Some(false));
    assert_eq!(
        label_of(&mut cx, "ai-off-note".into()).as_deref(),
        Some(strings::ai::OFF)
    );
    // The switch is all there is.
    assert_eq!(
        labels_in(&mut cx, "ai-section".into(), Role::CheckBox),
        [strings::ai::FEATURES]
    );
    assert_eq!(
        labels_in(&mut cx, "ai-section".into(), Role::Button),
        Vec::<String>::new()
    );
    for hidden in ["ai-providers", "ai-add", "ai-debug-log"] {
        assert!(!shown(&mut cx, hidden), "{hidden}");
    }
    assert_eq!(exclude_value(&mut cx), None);

    click(&mut cx, "ai-features".into());
    wait_until(&mut cx, |cx| {
        providers(cx) == ["Hosted · OpenAI-compatible · small-model"]
    });
    assert!(ai_view(&f).enabled);
    assert!(!shown(&mut cx, "ai-off-note"));
    for back in ["ai-add", "ai-debug-log", "ai-debug-log-note"] {
        assert!(shown(&mut cx, back), "{back}");
    }
    assert!(exclude_value(&mut cx).is_some());
}

/// AI-4 and AI-7: the exclude list and AI debug logging save through
/// `ai_settings_set`.
#[gpui::test]
fn ai_4_ai_7_exclude_list_and_debug_logging_are_saved(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    let defaults = ai_view(&f).exclude;
    assert_eq!(exclude_value(&mut cx), Some(defaults.join("\n")));
    assert_eq!(
        label_of(&mut cx, "ai-debug-log-note".into()).as_deref(),
        Some(strings::ai::DEBUG_LOG_NOTE)
    );

    // The field fills its frame, so a click there focuses it.
    click(&mut cx, "ai-exclude".into());
    press(&mut cx, &secondary("a"));
    cx.update(|window, cx| window.input("*.secret\n.env*", cx));
    cx.run_until_parked();
    // Leaving the field saves it.
    click(&mut cx, "ai-debug-log".into());
    wait_until(&mut cx, |_| {
        let view = ai_view(&f);
        view.debug_log && view.exclude == ["*.secret", ".env*"]
    });
    // The saved view reaches the window after the core keeps it.
    wait_until(&mut cx, |cx| checked(cx, "ai-debug-log") == Some(true));
}

/// A second workspace, "Java", with no repos; `mine` stays active.
fn second_tab(f: &Fixture, mine: &WorkspaceId) -> WorkspaceId {
    let app = f.app().clone();
    let java = f.runtime.block_on(async {
        let tabs = app.workspace_create("Java".to_owned()).await.unwrap();
        tabs.active.unwrap()
    });
    f.runtime
        .block_on(app.workspace_activate(mine.clone()))
        .unwrap();
    java
}

/// What tab `id` holds as the AI features value.
fn ai_on_in(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, id: &WorkspaceId) -> bool {
    cx.update(|_, cx| {
        let changes = window.read(cx).changes_of(id).expect("no changes pane");
        changes.read(cx).ai_enabled(cx)
    })
}

#[gpui::test]
fn ai_16_off_reaches_every_open_tab(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    let mine = f.active();
    let java = second_tab(&f, &mine);
    let (mut cx, window) = main_window(cx, &f);
    // Both tabs have been shown, so both have their panes.
    wait_for_element(&mut cx, tab(&java));
    click(&mut cx, tab(&java));
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| window.read(cx).changes_of(&java).is_some())
    });
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| window.read(cx).changes_of(&mine).is_some())
    });
    assert!(ai_on_in(&mut cx, &window, &mine));
    assert!(ai_on_in(&mut cx, &window, &java));

    open_settings(&mut cx);
    click(&mut cx, "ai-features".into());
    wait_until(&mut cx, |cx| {
        !ai_on_in(cx, &window, &mine) && !ai_on_in(cx, &window, &java)
    });

    // From the core's event, wherever the change came from (ARCH-6).
    set_options(&f, true);
    wait_until(&mut cx, |cx| {
        ai_on_in(cx, &window, &mine) && ai_on_in(cx, &window, &java)
    });
    assert_eq!(checked(&mut cx, "ai-features"), Some(true));
}

#[gpui::test]
fn sec_1_key_field_never_shows_the_key(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    save(
        &f,
        draft("Hosted", ProviderKind::Anthropic),
        Some(OLDER_KEY),
    );
    let (mut cx, window) = main_window(cx, &f);
    open_settings(&mut cx);
    click(&mut cx, named("ai-provider-edit", 0));
    wait_for_element(&mut cx, "provider-form".into());
    assert_eq!(
        label_of(&mut cx, "provider-form".into()),
        Some(strings::ai::provider("Hosted"))
    );

    // A stored key shows as such, never as text or a value.
    assert_eq!(
        label_of(&mut cx, "provider-key-stored".into()).as_deref(),
        Some(strings::ai::KEY_STORED)
    );
    assert!(!shown(&mut cx, "provider-key-field"));
    assert!(!rendered_text(&mut cx).contains(OLDER_KEY));

    click(&mut cx, "provider-key-replace".into());
    wait_for_element(&mut cx, "provider-key-field".into());
    let dialog = settings_dialog(&mut cx, &window).unwrap();
    let field = cx.update(|_, cx| dialog.read(cx).key_field(cx)).unwrap();
    cx.update(|_, cx| {
        assert!(field.read(cx).presentation().is_masked());
        assert_eq!(field.read(cx).value(), "");
    });
    let field = field.downgrade();
    type_into(&mut cx, "provider-key-field", KEY);
    click(&mut cx, "provider-save".into());
    wait_until(&mut cx, |cx| !shown(cx, "provider-form"));
    assert_eq!(f.keychain_secrets(), [KEY]);

    // The closed form's field is gone, or emptied.
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    let left = cx.update(|_, cx| field.upgrade().map(|field| field.read(cx).value()));
    assert!(left.as_ref().is_none_or(|text| text.is_empty()), "{left:?}");

    // Edited again: "Key stored", and the key is nowhere on screen nor in
    // any view the dialog keeps.
    click(&mut cx, named("ai-provider-edit", 0));
    wait_for_element(&mut cx, "provider-key-stored".into());
    let text = rendered_text(&mut cx);
    assert!(!text.contains(KEY) && !text.contains(OLDER_KEY), "{text}");
    assert!(cx.update(|_, cx| dialog.read(cx).key_field(cx)).is_none());
    let view = format!("{:?}", ai_view(&f));
    assert!(!view.contains(KEY), "{view}");
}

#[gpui::test]
fn sec_3_provider_names_and_models_render_as_text(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    let name = "<b>Bold</b> & \"co\"";
    let model = "<script>alert(1)</script>";
    save(
        &f,
        ProviderDraft {
            model: Some(model.to_owned()),
            ..draft(name, ProviderKind::Openai)
        },
        None,
    );
    let (mut cx, _) = main_window(cx, &f);
    open_settings(&mut cx);
    wait_for_element(&mut cx, named("ai-provider-name", 0));
    assert_eq!(
        label_of(&mut cx, named("ai-provider-name", 0)).as_deref(),
        Some(name)
    );
    assert_eq!(
        providers(&mut cx),
        [format!("{name} · OpenAI-compatible · {model}")]
    );
    assert_eq!(
        label_of(&mut cx, named("ai-provider-edit", 0)),
        Some(strings::ai::edit_label(name))
    );
}

#[gpui::test]
fn ui_22_the_settings_dialog_is_keyboard_reachable(cx: &mut TestAppContext) {
    let f = start(AiScript::default());
    save(&f, draft("Hosted", ProviderKind::Openai), None);
    let (mut cx, window) = main_window(cx, &f);
    open_settings(&mut cx);

    // The focus starts on the tab's first control.
    wait_until(&mut cx, |cx| focused(cx, "ai-features"));
    assert!(selected(&mut cx, "settings-tab-ai".into()));
    // Back to the tab strip; the arrows, Home and End stay on the one tab.
    press(&mut cx, "shift-tab");
    assert!(focused(&mut cx, "settings-tab-ai"));
    for key in ["right", "left", "home", "end"] {
        press(&mut cx, key);
        assert!(focused(&mut cx, "settings-tab-ai"), "{key}");
        assert!(selected(&mut cx, "settings-tab-ai".into()), "{key}");
    }
    press(&mut cx, "tab");
    assert!(focused(&mut cx, "ai-features"));

    // Tab reaches the list's buttons and Add provider.
    let mut reached = Vec::new();
    for _ in 0..12 {
        press(&mut cx, "tab");
        for id in [
            named("ai-provider-edit", 0),
            named("ai-provider-default", 0),
            named("ai-provider-remove", 0),
            "ai-add".into(),
        ] {
            if focused(&mut cx, id.clone()) && !reached.contains(&id) {
                reached.push(id);
            }
        }
        if focused(&mut cx, "ai-add") {
            break;
        }
    }
    assert_eq!(reached.len(), 4, "{reached:?}");

    // Enter opens the form, with the focus on its first control.
    press(&mut cx, "enter");
    wait_for_element(&mut cx, "provider-form".into());
    assert!(focused(&mut cx, named("provider-kind", 0)));
    press(&mut cx, "tab");
    press(&mut cx, "tab");
    assert!(focused(&mut cx, named("provider-kind", 2)));
    press(&mut cx, "enter");
    assert!(selected(&mut cx, named("provider-kind", 2)));
    assert!(shown(&mut cx, "provider-path"));

    // Opening Settings again keeps the one dialog and focuses it.
    cx.update(|window_cx, cx| {
        window.update(cx, |window, cx| {
            window.open_settings(SettingsTab::Ai, window_cx, cx)
        })
    });
    cx.run_until_parked();
    assert!(shown(&mut cx, "provider-form"));
    let dialog = settings_dialog(&mut cx, &window).unwrap();

    // Escape closes the dialog.
    press(&mut cx, "escape");
    wait_until(&mut cx, |cx| !shown(cx, "settings-dialog"));
    assert!(settings_dialog(&mut cx, &window).is_none());
    drop(dialog);
}
