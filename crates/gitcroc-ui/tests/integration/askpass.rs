//! Git's prompts in the window (D10): a fetch from a host that wants a
//! password asks in the askpass dialog, with `gitcroc-next` as git's
//! helper, and the answer goes to git. The dialog empties its field and is
//! dropped once it has answered, so nothing keeps what was typed (SEC-1).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_app::askpass::{AskpassPrompt, PromptKind};
use gitcroc_core::{Outcome, SkipReason};
use gitcroc_ui::askpass::AskpassDialog;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::history::{click, id_of, repo, wait_until};
use crate::jobs::{fetch, job, label_of, made, press, wait_for_end};
use crate::support::{Fixture, main_window, open};

const PASSWORD: &str = "s3cret-for-askpass";

/// The `Authorization` header of each request a host got.
type Seen = Arc<Mutex<Vec<Option<String>>>>;

/// A host that wants a password and refuses every answer. Returns a repo
/// URL on it, and what it was sent.
fn refusing_host(fixture: &Fixture) -> (String, Seen) {
    let listener = fixture
        .runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let url = format!("http://{}/api.git", listener.local_addr().unwrap());
    let seen = Seen::default();
    let kept = seen.clone();
    fixture.runtime.spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let kept = kept.clone();
            tokio::spawn(async move {
                let mut stream = tokio::io::BufReader::new(stream);
                let mut authorization = None;
                loop {
                    let mut line = String::new();
                    match stream.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) if line == "\r\n" => break,
                        Ok(_) => {}
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("authorization")
                    {
                        authorization = Some(value.trim().to_owned());
                    }
                }
                kept.lock().unwrap().push(authorization);
                let _ = stream
                    .get_mut()
                    .write_all(
                        b"HTTP/1.1 401 Unauthorized\r\n\
                          WWW-Authenticate: Basic realm=\"gitcroc\"\r\n\
                          Content-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            });
        }
    });
    (url, seen)
}

/// Waits until the window shows a prompt other than `after`.
fn wait_for_dialog(
    cx: &mut VisualTestContext,
    window: &Entity<GitcrocWindow>,
    after: Option<u32>,
) -> Entity<AskpassDialog> {
    let mut shown = None;
    wait_until(cx, |cx| {
        shown = cx.update(|_, cx| {
            window
                .read(cx)
                .askpass_dialog()
                .filter(|dialog| Some(dialog.read(cx).prompt_id()) != after)
        });
        shown.is_some()
    });
    shown.unwrap()
}

fn prompt_text(cx: &mut VisualTestContext) -> String {
    label_of(cx, "askpass-prompt").expect("no prompt")
}

fn answer(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| window.input(text, cx));
    cx.run_until_parked();
    press(cx, "enter");
}

/// D10: the dialog names the job, the repo and the workspace; the user
/// name is shown as typed, the password masked, and both reach the host.
/// The answered dialog is dropped, and its field emptied.
#[gpui::test]
fn d10_askpass_dialog_answers_git(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (url, seen) = refusing_host(&fixture);
    let made = made(&url);
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, window) = main_window(cx, &fixture);

    let id = fetch(&fixture, &workspace, &id_of(&added, "api"));
    let first = wait_for_dialog(&mut cx, &window, None);
    let label = job(&fixture, id).label;
    assert_eq!(
        label_of(&mut cx, "askpass-dialog"),
        Some(format!("{label}: api in My workspace"))
    );
    assert!(prompt_text(&mut cx).starts_with("Username for "));
    let first_id = cx.update(|_, cx| {
        assert!(!first.read(cx).masked(cx));
        first.read(cx).prompt_id()
    });
    drop(first);
    answer(&mut cx, "alice");

    let second = wait_for_dialog(&mut cx, &window, Some(first_id));
    assert!(prompt_text(&mut cx).starts_with("Password for "));
    let (dialog, field) = cx.update(|_, cx| {
        assert!(second.read(cx).masked(cx));
        (second.downgrade(), second.read(cx).field().downgrade())
    });
    drop(second);
    answer(&mut cx, PASSWORD);
    // The host refuses the password too, so the fetch fails.
    wait_for_end(&mut cx, &fixture, id, &Outcome::Failed);
    // base64 of `alice:s3cret-for-askpass`.
    let sent = Some("Basic YWxpY2U6czNjcmV0LWZvci1hc2twYXNz".to_owned());
    assert!(seen.lock().unwrap().contains(&sent), "{seen:?}");

    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    assert!(dialog.upgrade().is_none(), "the answered dialog is kept");
    // GPUI's test window may still hold the field; its text is gone.
    let left = cx.update(|_, cx| field.upgrade().map(|field| field.read(cx).value()));
    assert!(left.as_ref().is_none_or(|text| text.is_empty()), "{left:?}");
}

/// D10: Escape gives git no answer, and the fetch fails with git's
/// message.
#[gpui::test]
fn d10_cancelled_askpass_dialog_fails_the_job(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = made(&refusing_host(&fixture).0);
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, window) = main_window(cx, &fixture);

    let id = fetch(&fixture, &workspace, &id_of(&added, "api"));
    wait_for_dialog(&mut cx, &window, None);
    press(&mut cx, "escape");
    wait_for_end(&mut cx, &fixture, id, &Outcome::Failed);
    assert!(!job(&fixture, id).output.is_empty());
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| window.read(cx).askpass_dialog().is_none())
    });
}

/// D10: when its job ends, as when it is cancelled, its prompt goes.
#[gpui::test]
fn d10_askpass_dialog_closes_when_its_job_ends(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let made = made(&refusing_host(&fixture).0);
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, window) = main_window(cx, &fixture);

    let id = fetch(&fixture, &workspace, &id_of(&added, "api"));
    wait_for_dialog(&mut cx, &window, None);
    fixture.app().job_cancel(id);
    let cancelled = Outcome::Skipped {
        reason: SkipReason::Cancelled,
    };
    wait_for_end(&mut cx, &fixture, id, &cancelled);
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| window.read(cx).askpass_dialog().is_none())
    });
    assert_eq!(label_of(&mut cx, "askpass-dialog"), None);
}

/// D10: two fetches ask at once; their prompts are shown one after the
/// other.
#[gpui::test]
fn d10_askpass_prompts_queue(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let (url, _) = refusing_host(&fixture);
    let made = made(&url);
    let web = repo(&made, "web");
    web.git(["remote", "add", "origin", &url]).unwrap();
    web.git(["config", "credential.helper", ""]).unwrap();
    web.git(["config", "http.proxy", ""]).unwrap();
    let workspace = fixture.active();
    let added = fixture.add_repos(&workspace, made.path());
    let (mut cx, window) = main_window(cx, &fixture);

    let api = fetch(&fixture, &workspace, &id_of(&added, "api"));
    let web = fetch(&fixture, &workspace, &id_of(&added, "web"));
    let first = wait_for_dialog(&mut cx, &window, None);
    let first_id = cx.update(|_, cx| first.read(cx).prompt_id());
    let first_title = label_of(&mut cx, "askpass-dialog").unwrap();
    press(&mut cx, "escape");
    wait_for_dialog(&mut cx, &window, Some(first_id));
    let second_title = label_of(&mut cx, "askpass-dialog").unwrap();
    // No fetch is left waiting on git's helper when the app goes: on
    // Windows the helper would wait for its answer for ever.
    press(&mut cx, "escape");
    wait_for_end(&mut cx, &fixture, api, &Outcome::Failed);
    wait_for_end(&mut cx, &fixture, web, &Outcome::Failed);

    let mut titles = [first_title, second_title];
    titles.sort();
    assert!(titles[0].contains(": api in "), "{titles:?}");
    assert!(titles[1].contains(": web in "), "{titles:?}");
}

type Answers = Rc<RefCell<Vec<(u32, Option<String>)>>>;

/// A dialog of `prompt` alone, whose answers are kept.
fn dialog_of(cx: &mut TestAppContext, prompt: AskpassPrompt) -> (VisualTestContext, Answers) {
    let answers = Answers::default();
    let kept = answers.clone();
    let (cx, _) = open(cx, move |window, cx| {
        let send = Box::new(move |id, answer, _: &mut gpui_kit::App| {
            kept.borrow_mut().push((id, answer));
        });
        cx.new(|cx| AskpassDialog::new(prompt, send, window, cx))
    });
    (cx, answers)
}

fn prompt(id: u32, kind: PromptKind, text: &str) -> AskpassPrompt {
    AskpassPrompt {
        prompt_id: id,
        kind,
        text: text.to_owned(),
        workspace_name: "My workspace".to_owned(),
        repo_names: vec!["api".to_owned()],
        job_label: "Fetch api".to_owned(),
    }
}

/// D10: an SSH host key question is answered Yes or No.
#[gpui::test]
fn d10_host_key_question_is_yes_or_no(app: &mut TestAppContext) {
    let question = "Are you sure you want to continue connecting (yes/no/[fingerprint])?";
    let (mut cx, answers) = dialog_of(app, prompt(7, PromptKind::YesNo, question));
    assert_eq!(label_of(&mut cx, "askpass-field"), None);
    click(&mut cx, "askpass-yes".into());
    assert_eq!(*answers.borrow(), [(7, Some("yes".to_owned()))]);

    let (mut cx, answers) = dialog_of(app, prompt(8, PromptKind::YesNo, question));
    click(&mut cx, "askpass-no".into());
    assert_eq!(*answers.borrow(), [(8, Some("no".to_owned()))]);
}

/// SEC-3: git's prompt shows as written, never as markup.
#[gpui::test]
fn sec_3_askpass_prompt_renders_as_text(cx: &mut TestAppContext) {
    let text = "<b>Password</b> for 'https://<i>host</i>':";
    let (mut cx, _) = dialog_of(cx, prompt(1, PromptKind::Secret, text));
    assert_eq!(label_of(&mut cx, "askpass-prompt").as_deref(), Some(text));
}
