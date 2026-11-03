//! M6-23b: Resolve with AI in the conflict editor, the consent dialog and
//! the workspace switch (AI-3, AI-4, AI-5, AI-6, AI-9, AI-11, AI-12, AI-14,
//! AI-16, SEC-3, UI-22), the port of `ui/src/conflict/ai.test.tsx` and
//! `ui/e2e/conflict.spec.ts`'s AI flow. Real git conflicts (QA-2); the
//! providers are the fixture's scripted fake with its mock provider, so no
//! test reaches a network or a CLI, and a held reply is released by the
//! test, never waited for by time.

use std::path::PathBuf;

use gitcroc_ai::{ProviderReply, Usage};
use gitcroc_app::ai::AiView;
use gitcroc_core::{AiOptions, Outcome, ProviderDraft, ProviderId, ProviderKind, WorkspaceId};
use gitcroc_ui::conflict::{ConflictEditor, Take};
use gitcroc_ui::settings_dialog::{SettingsDialog, SettingsTab};
use gitcroc_ui::strings::{ai as ai_text, conflict as text, tabs as tab_text};
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Entity, TestAppContext, VisualTestContext};

use crate::conflict_banner::repos;
use crate::conflict_flow::{
    job_end, mark_resolved, merge_conflict, on_disk, take, value, wait_for_editor,
};
use crate::history::{
    add, click, label_of, press, repo, right_click, tab, wait_for_element, wait_until,
};
use crate::stage::{disabled, focused, row_of, show_uncommitted};
use crate::support::{AiBackend, AiScript, Fixture, Options, main_window};

fn el(id: &str) -> ElementId {
    ElementId::Name(id.to_owned().into())
}

fn named(id: &str, at: usize) -> ElementId {
    ElementId::NamedInteger(id.to_owned().into(), at as u64)
}

fn start(script: AiScript) -> Fixture {
    Fixture::start_with(Options {
        ai: AiBackend::Script(AiScript {
            mock: true,
            ..script
        }),
        ..Options::default()
    })
}

fn reply(text: &str, usage: Option<(u32, u32)>) -> ProviderReply {
    ProviderReply {
        text: text.to_owned(),
        usage: usage.map(|(input_tokens, output_tokens)| Usage {
            input_tokens,
            output_tokens,
        }),
    }
}

/// A valid reply resolving hunks `ids` with `resolutions`, high confidence.
fn resolving(hunks: &[(u32, &str)]) -> ProviderReply {
    let hunks: Vec<String> = hunks
        .iter()
        .map(|(id, resolution)| {
            serde_json::json!({
                "id": id,
                "status": "resolved",
                "resolution": resolution,
                "explanation": "Kept both sides.",
                "confidence": "high",
            })
            .to_string()
        })
        .collect();
    reply(&format!("{{\"hunks\": [{}]}}", hunks.join(",")), None)
}

fn draft(name: &str, kind: ProviderKind) -> ProviderDraft {
    let api = matches!(kind, ProviderKind::Anthropic | ProviderKind::Openai);
    ProviderDraft {
        id: None,
        name: name.to_owned(),
        kind,
        model: api.then(|| "test-model".to_owned()),
        base_url: None,
        path: None,
        context_budget: None,
    }
}

fn ai_view(f: &Fixture) -> AiView {
    let app = f.app().clone();
    f.runtime
        .block_on(async move { app.ai_providers_get().await })
        .unwrap()
}

/// Saves `draft` with `key` and makes it the default; returns its id.
fn save_default(f: &Fixture, draft: ProviderDraft, key: Option<&str>) -> ProviderId {
    let app = f.app().clone();
    let key = key.map(str::to_owned);
    let name = draft.name.clone();
    let view = f
        .runtime
        .block_on(async move { app.ai_provider_save(draft, key).await })
        .unwrap();
    let id = view
        .providers
        .iter()
        .rev()
        .find(|provider| provider.name == name)
        .unwrap()
        .id
        .clone();
    let app = f.app().clone();
    let default = id.clone();
    f.runtime
        .block_on(async move { app.ai_provider_set_default(default).await })
        .unwrap();
    id
}

/// The mock provider, the default, consented to when `consent` is set.
fn mock_provider(f: &Fixture, name: &str, consent: bool) -> ProviderId {
    let id = save_default(f, draft(name, ProviderKind::Mock), None);
    if consent {
        let app = f.app().clone();
        let consented = id.clone();
        f.runtime
            .block_on(async move { app.ai_provider_consent(consented, "mock".to_owned()).await })
            .unwrap();
    }
    id
}

fn set_options(f: &Fixture, change: impl FnOnce(&mut AiOptions)) {
    let view = ai_view(f);
    let mut options = AiOptions {
        enabled: view.enabled,
        exclude: view.exclude,
        debug_log: view.debug_log,
    };
    change(&mut options);
    let app = f.app().clone();
    f.runtime
        .block_on(async move { app.ai_settings_set(options).await })
        .unwrap();
}

fn workspace_ai(f: &Fixture, id: &WorkspaceId) -> bool {
    f.app()
        .workspace_get()
        .open
        .into_iter()
        .find(|view| &view.id == id)
        .unwrap()
        .ai
}

fn shown(cx: &mut VisualTestContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

fn gone(cx: &mut VisualTestContext, id: &str) {
    let id = el(id);
    wait_until(cx, |cx| !shown(cx, id.clone()));
}

/// The texts of the numbered elements `id`, in order.
fn numbered_labels(cx: &mut VisualTestContext, id: &str) -> Vec<String> {
    (0..).map_while(|at| label_of(cx, named(id, at))).collect()
}

/// A repo `api` with `paths` in a merge conflict, its changes shown.
struct Open {
    f: Fixture,
    _made: repo_gen::Fixture,
    r: repo_gen::Repo,
    cx: VisualTestContext,
    window: Entity<GitcrocWindow>,
}

fn open_with(
    root: &mut TestAppContext,
    f: Fixture,
    conflict: impl FnOnce(&repo_gen::Repo),
) -> Open {
    let made = repos(&["api"]);
    let r = repo(&made, "api");
    conflict(&r);
    let added = add(&f, &made);
    let (mut cx, window) = main_window(root, &f);
    show_uncommitted(&mut cx, &window, &added, "api");
    Open {
        f,
        _made: made,
        r,
        cx,
        window,
    }
}

fn open(root: &mut TestAppContext, f: Fixture, paths: &[&str]) -> Open {
    open_with(root, f, |r| merge_conflict(r, paths))
}

/// 200 lines with two hunks far apart: line 5 says `ONE` and line 150
/// `TWO`, each side with its own word.
fn two_hunks(side: &str) -> String {
    (1..=200)
        .map(|n| match n {
            5 => format!("{side} ONE\n"),
            150 => format!("{side} TWO\n"),
            n => format!("line {n}\n"),
        })
        .collect()
}

fn two_hunk_conflict(r: &repo_gen::Repo) {
    r.write("a.txt", two_hunks("base")).unwrap();
    r.commit_all("Base").unwrap();
    r.branch("topic").unwrap();
    r.write("a.txt", two_hunks("main")).unwrap();
    r.commit_all("Main edit").unwrap();
    r.git(["checkout", "topic"]).unwrap();
    r.write("a.txt", two_hunks("topic")).unwrap();
    r.commit_all("Topic edit").unwrap();
    r.git(["checkout", "main"]).unwrap();
    assert!(r.git(["merge", "topic"]).is_err());
}

/// Clicks Resolve with AI once it can be pressed.
fn resolve(cx: &mut VisualTestContext) {
    wait_until(cx, |cx| {
        shown(cx, el("conflict-resolve-ai")) && !disabled(cx, el("conflict-resolve-ai"))
    });
    click(cx, el("conflict-resolve-ai"));
}

fn row_click(cx: &mut VisualTestContext, window: &Entity<GitcrocWindow>, path: &str) {
    let row = row_of(cx, window, gitcroc_ui::changes::List::Unmerged, path);
    click(cx, crate::changes::row(row));
}

/// The proposed hunks the editor holds.
fn proposed(cx: &mut VisualTestContext, editor: &Entity<ConflictEditor>) -> usize {
    cx.update(|_, cx| editor.read(cx).proposed_hunks().len())
}

/// Waits for the end of the next job.
fn ended(
    f: &Fixture,
    cx: &mut VisualTestContext,
    act: impl FnOnce(&mut VisualTestContext),
) -> Option<Outcome> {
    let mut events = f.app().subscribe();
    act(cx);
    job_end(cx, &mut events)
}

/// Opens tab `id`'s menu and returns whether "AI for this workspace" is
/// in it, and checked.
fn tab_menu_ai(cx: &mut VisualTestContext, id: &WorkspaceId) -> Option<bool> {
    right_click(cx, tab(id));
    wait_for_element(cx, "menu".into());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("menu")
            .try_find_by_label(tab_text::AI)
            .map(|found| found.checked() == Some(true))
    })
}

/// AI-3: with no provider configured, Resolve with AI opens Settings at its
/// AI tab.
#[gpui::test]
fn ai_3_no_provider_opens_the_settings(root: &mut TestAppContext) {
    let Open {
        f: _f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, start(AiScript::default()), &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_for_element(&mut cx, "settings-dialog".into());
    let at = cx.update(|_, cx| {
        window
            .read(cx)
            .dialog_of::<SettingsDialog>()
            .map(|dialog| dialog.read(cx).tab())
    });
    assert_eq!(at, Some(SettingsTab::Ai));
    wait_for_element(&mut cx, "ai-add".into());
}

/// AI-3: while the job runs the button is Cancel with the provider's name;
/// Cancel ends the job with no proposal.
#[gpui::test]
fn ai_3_while_the_job_runs_the_button_is_cancel_with_the_providers_name(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        held: true,
        ..AiScript::default()
    });
    mock_provider(&f, "Scripted", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_until(&mut cx, |_| !f.ai_gates().is_empty());
    let gate = f.ai_gates().remove(0);
    f.runtime.block_on(gate.arrived());
    wait_for_element(&mut cx, el("conflict-cancel-ai"));
    assert_eq!(
        label_of(&mut cx, el("conflict-cancel-ai")),
        Some(text::cancel_ai("Scripted"))
    );
    assert!(!shown(&mut cx, el("conflict-resolve-ai")));

    let outcome = ended(&f, &mut cx, |cx| click(cx, el("conflict-cancel-ai")));
    assert!(
        matches!(outcome, Some(Outcome::Skipped { .. })),
        "{outcome:?}"
    );
    wait_for_element(&mut cx, el("conflict-resolve-ai"));
    assert!(!shown(&mut cx, el("conflict-cancel-ai")));
    assert_eq!(proposed(&mut cx, &editor), 0);
    assert!(!shown(&mut cx, el("conflict-proposal-0")));
    assert!(!shown(&mut cx, el("conflict-ai-failed")));
}

/// AI-4: a file on the exclude list is not sent; the reason shows beside
/// the disabled button before any press, and goes with the pattern.
#[gpui::test]
fn ai_4_the_ineligible_reason_shows_beside_the_disabled_button(root: &mut TestAppContext) {
    let f = start(AiScript::default());
    mock_provider(&f, "Mock", true);
    set_options(&f, |options| options.exclude = vec!["*.txt".to_owned()]);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    let reason = text::not_sent_to_ai("matches the exclude list (*.txt)");
    wait_until(&mut cx, |cx| {
        label_of(cx, el("conflict-ai-reason")).as_deref() == Some(reason.as_str())
    });
    assert!(disabled(&mut cx, el("conflict-resolve-ai")));

    // A new exclude list is asked about again.
    set_options(&f, |options| options.exclude = vec!["*.key".to_owned()]);
    gone(&mut cx, "conflict-ai-reason");
    assert!(!disabled(&mut cx, el("conflict-resolve-ai")));
}

/// AI-5: with one hunk taken, only the other is sent.
#[gpui::test]
fn ai_5_only_the_untouched_hunks_are_sent(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(2, "both TWO\n")])],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open_with(root, f, two_hunk_conflict);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    take(&mut cx, &editor, Take::Ours);
    let outcome = ended(&f, &mut cx, resolve);
    assert_eq!(outcome, Some(Outcome::Succeeded));
    let requests = f.ai_requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let sent = &requests[0].user;
    assert!(sent.contains("topic TWO"), "{sent}");
    assert!(!sent.contains("topic ONE"), "{sent}");
}

/// AI-6: the consent dialog says what is sent and to which host, or which
/// program, in the core's words.
#[gpui::test]
fn ai_6_consent_dialog_names_what_is_sent_and_where(root: &mut TestAppContext) {
    let f = start(AiScript::default());
    let api = save_default(
        &f,
        draft("Claude API", ProviderKind::Anthropic),
        Some("sk-key"),
    );
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    let request = |f: &Fixture, id: &ProviderId| {
        ai_view(f)
            .providers
            .into_iter()
            .find(|provider| &provider.id == id)
            .and_then(|provider| provider.consent_needed)
            .unwrap()
    };

    let asked = request(&f, &api);
    let host = asked.to.value().to_owned();
    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    assert_eq!(
        label_of(&mut cx, el("dialog-title")),
        Some(ai_text::consent::title(&host))
    );
    assert_eq!(label_of(&mut cx, el("consent-target")), Some(host.clone()));
    assert_eq!(
        label_of(&mut cx, el("consent-text")),
        Some(ai_text::consent::text(&asked))
    );
    assert!(
        ai_text::consent::text(&asked).contains(&ai_text::consent::sends_to_host("Claude API"))
    );
    assert_eq!(numbered_labels(&mut cx, "consent-sent-item"), asked.sent);
    assert_eq!(asked.sent.len(), 5);
    click(&mut cx, el("consent-cancel"));
    gone(&mut cx, "consent-dialog");

    let path = if cfg!(windows) {
        PathBuf::from(r"C:\Tools\claude.exe")
    } else {
        PathBuf::from("/opt/tools/claude")
    };
    let cli = save_default(
        &f,
        ProviderDraft {
            path: Some(path.clone()),
            ..draft("Claude Code", ProviderKind::ClaudeCode)
        },
        None,
    );
    let asked = request(&f, &cli);
    assert_eq!(asked.to.value(), path.display().to_string());
    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    assert_eq!(
        label_of(&mut cx, el("consent-target")),
        Some(path.display().to_string())
    );
    let said = label_of(&mut cx, el("consent-text")).unwrap();
    assert!(
        said.contains(&ai_text::consent::runs_program("Claude Code")),
        "{said}"
    );
    assert!(said.contains(ai_text::consent::PASSES_IT), "{said}");
    assert_eq!(numbered_labels(&mut cx, "consent-sent-item"), asked.sent);
    click(&mut cx, el("consent-cancel"));
    gone(&mut cx, "consent-dialog");
    assert!(f.ai_requests().is_empty());
}

/// AI-6: consent comes first and the request is asked again after Allow;
/// Cancel sends nothing.
#[gpui::test]
fn ai_6_consent_first_then_asked_again_and_cancel_sends_nothing(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        ..AiScript::default()
    });
    let id = mock_provider(&f, "Mock", false);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    assert!(f.ai_requests().is_empty());
    click(&mut cx, el("consent-cancel"));
    gone(&mut cx, "consent-dialog");
    assert!(f.ai_requests().is_empty());
    assert!(!shown(&mut cx, el("conflict-cancel-ai")));

    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    let outcome = ended(&f, &mut cx, |cx| click(cx, el("consent-allow")));
    assert_eq!(outcome, Some(Outcome::Succeeded));
    gone(&mut cx, "consent-dialog");
    assert_eq!(f.ai_requests().len(), 1);
    let provider = ai_view(&f)
        .providers
        .into_iter()
        .find(|provider| provider.id == id)
        .unwrap();
    assert_eq!(provider.consent.as_deref(), Some("mock"));
    assert_eq!(provider.consent_needed, None);
    wait_until(&mut cx, |cx| proposed(cx, &editor) == 1);
}

/// AI-6: a provider whose host changed asks again, naming the new host.
#[gpui::test]
fn ai_6_consent_asked_again_when_the_host_changes(root: &mut TestAppContext) {
    let f = start(AiScript::default());
    let local = |url: &str| ProviderDraft {
        base_url: Some(url.to_owned()),
        ..draft("Local", ProviderKind::Openai)
    };
    let id = save_default(&f, local("http://localhost:11434/v1"), None);
    let app = f.app().clone();
    let consented = id.clone();
    f.runtime
        .block_on(async move {
            app.ai_provider_consent(consented, "localhost:11434".to_owned())
                .await
        })
        .unwrap();
    let app = f.app().clone();
    let moved = ProviderDraft {
        id: Some(id),
        ..local("http://models.example:8080/v1")
    };
    f.runtime
        .block_on(async move { app.ai_provider_save(moved, None).await })
        .unwrap();
    let Open {
        f: _f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    assert_eq!(
        label_of(&mut cx, el("consent-target")),
        Some("models.example:8080".to_owned())
    );
    click(&mut cx, el("consent-cancel"));
    gone(&mut cx, "consent-dialog");
}

/// AI-6: AI for this workspace, in the tab menu, disables the button with
/// the reason, and turns it on again.
#[gpui::test]
fn ai_6_the_workspace_switch_disables_the_button_with_the_reason(root: &mut TestAppContext) {
    let f = start(AiScript::default());
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    let id = f.active();
    wait_for_element(&mut cx, el("conflict-resolve-ai"));
    assert!(!disabled(&mut cx, el("conflict-resolve-ai")));

    assert_eq!(tab_menu_ai(&mut cx, &id), Some(true));
    crate::history::click_menu(&mut cx, tab_text::AI);
    wait_until(&mut cx, |_| !workspace_ai(&f, &id));
    wait_until(&mut cx, |cx| {
        label_of(cx, el("conflict-ai-reason")).as_deref() == Some(text::AI_OFF_HERE)
    });
    assert!(disabled(&mut cx, el("conflict-resolve-ai")));

    assert_eq!(tab_menu_ai(&mut cx, &id), Some(false));
    crate::history::click_menu(&mut cx, tab_text::AI);
    wait_until(&mut cx, |_| workspace_ai(&f, &id));
    gone(&mut cx, "conflict-ai-reason");
    assert!(!disabled(&mut cx, el("conflict-resolve-ai")));
}

/// AI-9: two invalid replies mark the file "AI failed" with the reason,
/// and Try again asks again.
#[gpui::test]
fn ai_9_ai_failed_shows_the_reason_with_try_again(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![reply("not json", None), reply("still not json", None)],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    let outcome = ended(&f, &mut cx, resolve);
    assert_eq!(outcome, Some(Outcome::Failed));
    wait_for_element(&mut cx, el("conflict-ai-failed"));
    let failed = label_of(&mut cx, el("conflict-ai-failed")).unwrap();
    assert!(failed.starts_with("AI failed: "), "{failed}");
    assert!(!failed.starts_with("AI failed: AI failed"), "{failed}");
    // The invalid reply was retried once (AI-9).
    assert_eq!(f.ai_requests().len(), 2);
    assert_eq!(proposed(&mut cx, &editor), 0);
    assert!(!disabled(&mut cx, el("conflict-ai-retry")));

    let outcome = ended(&f, &mut cx, |cx| click(cx, el("conflict-ai-retry")));
    assert_eq!(outcome, Some(Outcome::Failed));
    assert_eq!(f.ai_requests().len(), 4);
    wait_for_element(&mut cx, el("conflict-ai-failed"));
}

/// AI-14: each request's token usage shows when the provider reported it,
/// and nothing for a request without.
#[gpui::test]
fn ai_14_token_usage_shown_per_request(root: &mut TestAppContext) {
    let valid = resolving(&[(1, "both\n")]);
    let f = start(AiScript {
        // An invalid reply with usage, then a valid one without.
        replies: vec![reply("not json", Some((1200, 300))), valid],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    assert_eq!(ended(&f, &mut cx, resolve), Some(Outcome::Succeeded));
    wait_for_element(&mut cx, el("conflict-ai-usage"));
    assert_eq!(
        numbered_labels(&mut cx, "conflict-ai-usage-line"),
        [text::request(1, &text::tokens(1200, 300))]
    );
    assert_eq!(
        text::tokens(1200, 300),
        "1,200 input tokens, 300 output tokens"
    );
}

/// AI-11: a live proposal reaches the cards, of the file shown and of a
/// kept file not shown, and survives a switch to another file and back.
#[gpui::test]
fn ai_11_a_live_proposal_reaches_the_cards(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        held: true,
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt", "b.txt"]);
    let a = wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_until(&mut cx, |_| !f.ai_gates().is_empty());
    let gate = f.ai_gates().remove(0);
    f.runtime.block_on(gate.arrived());

    // a.txt's job ends while b.txt is shown; a.txt's kept editor gets it.
    row_click(&mut cx, &window, "b.txt");
    wait_for_editor(&mut cx, &window, "b.txt");
    let mut events = f.app().subscribe();
    gate.release();
    assert_eq!(job_end(&mut cx, &mut events), Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| proposed(cx, &a) == 1);

    row_click(&mut cx, &window, "a.txt");
    wait_for_editor(&mut cx, &window, "a.txt");
    wait_for_element(&mut cx, el("conflict-proposal-0"));
    // No usage was reported: no usage list (AI-14).
    assert!(!shown(&mut cx, el("conflict-ai-usage")));
    wait_for_element(&mut cx, el("conflict-accept-high"));

    row_click(&mut cx, &window, "b.txt");
    wait_for_editor(&mut cx, &window, "b.txt");
    assert!(!shown(&mut cx, el("conflict-proposal-0")));
    row_click(&mut cx, &window, "a.txt");
    wait_for_editor(&mut cx, &window, "a.txt");
    wait_for_element(&mut cx, el("conflict-proposal-0"));
}

/// AI-12: Resolve with AI, accept both hunks, Mark resolved and Continue:
/// the file on disk has the accepted text.
#[gpui::test]
fn ai_12_resolve_accept_both_hunks_mark_resolved_and_continue(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both ONE\n"), (2, "both TWO\n")])],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        r,
        mut cx,
        window,
        _made,
        ..
    } = open_with(root, f, two_hunk_conflict);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    let before = on_disk(&r, "a.txt");
    assert_eq!(ended(&f, &mut cx, resolve), Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| proposed(cx, &editor) == 2);
    click(&mut cx, el("conflict-0-accept"));
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            assert!(editor.accept_proposal(1, window, cx))
        })
    });
    cx.run_until_parked();
    let accepted = (1..=200)
        .map(|n| match n {
            5 => "both ONE\n".to_owned(),
            150 => "both TWO\n".to_owned(),
            n => format!("line {n}\n"),
        })
        .collect::<String>();
    assert_eq!(value(&mut cx, &editor), accepted);
    // Accepting wrote nothing (AI-12).
    assert_eq!(on_disk(&r, "a.txt"), before);

    assert_eq!(mark_resolved(&f, &mut cx), Some(Outcome::Succeeded));
    assert_eq!(on_disk(&r, "a.txt"), accepted);
    wait_until(&mut cx, |cx| {
        !cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("conflict-banner-continue").disabled() == Some(true)
        })
    });
    let mut events = f.app().subscribe();
    click(&mut cx, "conflict-banner-continue".into());
    crate::conflict_banner::ended(&mut events, &mut cx);
    assert!(
        r.git(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
            .is_err()
    );
    assert_eq!(on_disk(&r, "a.txt"), accepted);
}

/// AI-16: AI features off takes the button and every proposal at once,
/// a shown one too; the buffer stays.
#[gpui::test]
fn ai_16_off_hides_resolve_with_ai_and_proposals(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", true);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    assert_eq!(ended(&f, &mut cx, resolve), Some(Outcome::Succeeded));
    wait_for_element(&mut cx, el("conflict-proposal-0"));
    let buffer = value(&mut cx, &editor);

    set_options(&f, |options| options.enabled = false);
    gone(&mut cx, "conflict-proposal-0");
    assert!(!shown(&mut cx, el("conflict-resolve-ai")));
    assert!(!shown(&mut cx, el("conflict-accept-high")));
    assert_eq!(proposed(&mut cx, &editor), 0);
    assert_eq!(value(&mut cx, &editor), buffer);

    set_options(&f, |options| options.enabled = true);
    wait_for_element(&mut cx, el("conflict-resolve-ai"));
    assert!(!shown(&mut cx, el("conflict-proposal-0")));
    assert_eq!(proposed(&mut cx, &editor), 0);
}

/// AI-16: AI features off hides AI for this workspace from the tab menu.
#[gpui::test]
fn ai_16_off_hides_the_workspace_ai_switch(root: &mut TestAppContext) {
    let f = start(AiScript::default());
    set_options(&f, |options| options.enabled = false);
    let (mut cx, window) = main_window(root, &f);
    let id = f.active();
    let heard = |cx: &mut VisualTestContext, on: bool| {
        cx.update(|_, cx| {
            let ai = window.read(cx).ai().read(cx);
            ai.view().is_some_and(|view| view.enabled == on)
        })
    };
    wait_until(&mut cx, |cx| heard(cx, false));
    assert_eq!(tab_menu_ai(&mut cx, &id), None);
    press(&mut cx, "escape");
    gone(&mut cx, "menu");

    set_options(&f, |options| options.enabled = true);
    wait_until(&mut cx, |cx| heard(cx, true));
    assert_eq!(tab_menu_ai(&mut cx, &id), Some(true));
}

/// AI-16: turning AI features off closes the consent dialog unanswered.
#[gpui::test]
fn ai_16_off_never_opens_the_consent_dialog(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", false);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    set_options(&f, |options| options.enabled = false);
    gone(&mut cx, "consent-dialog");
    assert!(f.ai_requests().is_empty());
    assert!(!shown(&mut cx, el("conflict-resolve-ai")));
}

/// SEC-3: the provider's name and the reason render as text.
#[gpui::test]
fn sec_3_the_reason_and_consent_target_render_as_text(root: &mut TestAppContext) {
    let name = "<b>Mock</b> & <i>co</i>";
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        held: true,
        ..AiScript::default()
    });
    mock_provider(&f, name, false);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt", "b.key.txt"]);
    wait_for_editor(&mut cx, &window, "a.txt");
    resolve(&mut cx);
    wait_for_element(&mut cx, "consent-dialog".into());
    let said = label_of(&mut cx, el("consent-text")).unwrap();
    assert!(
        said.contains(&ai_text::consent::sends_to_host(name)),
        "{said}"
    );
    assert_eq!(
        label_of(&mut cx, el("consent-target")),
        Some("mock".to_owned())
    );
    click(&mut cx, el("consent-allow"));
    gone(&mut cx, "consent-dialog");
    wait_until(&mut cx, |_| !f.ai_gates().is_empty());
    wait_for_element(&mut cx, el("conflict-cancel-ai"));
    assert_eq!(
        label_of(&mut cx, el("conflict-cancel-ai")),
        Some(text::cancel_ai(name))
    );
    ended(&f, &mut cx, |cx| click(cx, el("conflict-cancel-ai")));

    set_options(&f, |options| options.exclude = vec!["b.*".to_owned()]);
    row_click(&mut cx, &window, "b.key.txt");
    wait_for_editor(&mut cx, &window, "b.key.txt");
    let reason = text::not_sent_to_ai("matches the exclude list (b.*)");
    wait_until(&mut cx, |cx| {
        label_of(cx, el("conflict-ai-reason")).as_deref() == Some(reason.as_str())
    });
}

/// UI-22: Resolve with AI and the consent dialog work from the keyboard:
/// Tab reaches the button, Space presses it, the dialog starts on Cancel,
/// Escape cancels, and Tab and Space allow.
#[gpui::test]
fn ui_22_resolve_with_ai_and_the_consent_dialog_are_keyboard_reachable(root: &mut TestAppContext) {
    let f = start(AiScript {
        replies: vec![resolving(&[(1, "both\n")])],
        ..AiScript::default()
    });
    mock_provider(&f, "Mock", false);
    let Open {
        f,
        mut cx,
        window,
        _made,
        ..
    } = open(root, f, &["a.txt"]);
    let editor = wait_for_editor(&mut cx, &window, "a.txt");
    wait_for_element(&mut cx, el("conflict-resolve-ai"));
    crate::conflict_flow::focus_mark(&mut cx, &window, false);
    press(&mut cx, "tab");
    assert!(focused(&mut cx, el("conflict-resolve-ai")));
    press(&mut cx, "space");
    wait_for_element(&mut cx, "consent-dialog".into());
    assert!(focused(&mut cx, el("consent-cancel")));
    press(&mut cx, "escape");
    gone(&mut cx, "consent-dialog");
    assert!(focused(&mut cx, el("conflict-resolve-ai")));
    assert!(f.ai_requests().is_empty());

    press(&mut cx, "space");
    wait_for_element(&mut cx, "consent-dialog".into());
    press(&mut cx, "tab");
    assert!(focused(&mut cx, el("consent-allow")));
    let outcome = ended(&f, &mut cx, |cx| press(cx, "space"));
    assert_eq!(outcome, Some(Outcome::Succeeded));
    wait_until(&mut cx, |cx| proposed(cx, &editor) == 1);
}
