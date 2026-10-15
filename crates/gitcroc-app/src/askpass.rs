//! Prompts git and ssh cannot answer silently (D10, section 4
//! "Authentication prompts"). The helper is the gitcroc executable itself:
//! git starts it as `GIT_ASKPASS` or `SSH_ASKPASS` with
//! [`ADDRESS_VAR`] and [`TOKEN_VAR`] set, and in that mode it opens no
//! window. It sends git's prompt and the job's token to the running app
//! over a local socket ([`crate::platform::LocalSockets`]), prints the
//! answer on stdout and exits; a missing app, a token the app did not
//! issue, or a cancelled dialog make it exit with status 1, which git takes
//! as no answer.
//!
//! The app asks the user through an [`AskpassUi`]: a dialog of its window,
//! or the tests' fake. The answer goes to the helper and nowhere else: it is
//! never stored, logged or cached (D10, SEC-1).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write as _};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gitcroc_core::{Askpass, AskpassGrant, AskpassIssuer, JobView};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Notify, oneshot};

use crate::platform::{LocalListener, LocalSockets, PlatformSockets};

/// Set by the app for git: where the helper reaches the app.
pub const ADDRESS_VAR: &str = "GITCROC_ASKPASS";

/// Set by the app for git: the job's token, which the app checks.
pub const TOKEN_VAR: &str = "GITCROC_ASKPASS_TOKEN";

/// The longest request the app reads; git's prompts are one short line.
const MAX_REQUEST: u64 = 64 * 1024;

/// What git asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    /// A name or other text, shown as it is typed.
    Text,
    /// A password or passphrase, masked.
    Secret,
    /// An SSH host key question, answered `yes` or `no`.
    YesNo,
}

impl PromptKind {
    /// The kind of git's or ssh's prompt `text`.
    pub fn of(text: &str) -> Self {
        let text = text.to_lowercase();
        if text.contains("(yes/no") {
            Self::YesNo
        } else if text.contains("password") || text.contains("passphrase") {
            Self::Secret
        } else {
            Self::Text
        }
    }
}

/// A prompt as the dialog shows it, naming the repo and the job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AskpassPrompt {
    pub prompt_id: u32,
    pub kind: PromptKind,
    /// Git's or ssh's prompt, shown as text (SEC-3).
    pub text: String,
    pub workspace_name: String,
    pub repo_names: Vec<String>,
    pub job_label: String,
}

/// The future of an answer: `None` when the user cancelled.
pub type Answer<'a> = Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;

/// Asks the user (PLAT-1): the app's dialog, or a fake in the tests.
pub trait AskpassUi: Send + Sync {
    /// Asks `prompt`. Dropped before it resolves, as when the job ended
    /// meanwhile, the question is withdrawn.
    fn ask(&self, prompt: AskpassPrompt) -> Answer<'_>;
}

/// The helper's request, one JSON line.
#[derive(Serialize, Deserialize)]
struct Request {
    token: String,
    prompt: String,
}

/// The app's reply, one JSON line. `None` is no answer.
#[derive(Serialize, Deserialize)]
struct Reply {
    answer: Option<String>,
}

/// A job whose prompts may be asked: what the dialog names, and the
/// signal that it ended.
struct Issued {
    workspace_name: String,
    repo_names: Vec<String>,
    job_label: String,
    revoked: Arc<Notify>,
}

type Tokens = Mutex<HashMap<String, Issued>>;

fn lock(tokens: &Tokens) -> MutexGuard<'_, HashMap<String, Issued>> {
    // Each change is one insert or removal.
    tokens.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The app's end: issues tokens to network jobs and answers the helper.
pub struct AskpassServer {
    program: PathBuf,
    address: String,
    ui: Arc<dyn AskpassUi>,
    tokens: Arc<Tokens>,
    prompt_ids: AtomicU32,
}

impl std::fmt::Debug for AskpassServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AskpassServer")
            .field("program", &self.program)
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl AskpassServer {
    /// Serves the helper `program` on `listener`, asking through `ui`.
    /// Needs a tokio runtime.
    pub fn start<L: LocalListener>(
        mut listener: L,
        program: PathBuf,
        ui: Arc<dyn AskpassUi>,
    ) -> Arc<Self> {
        let server = Arc::new(Self {
            program,
            address: listener.address().to_owned(),
            ui,
            tokens: Arc::default(),
            prompt_ids: AtomicU32::new(1),
        });
        let serving = Arc::downgrade(&server);
        tokio::spawn(async move {
            // Ends with the server, or when the socket fails.
            while let Ok(stream) = listener.accept().await {
                let Some(server) = serving.upgrade() else {
                    return;
                };
                tokio::spawn(async move { server.serve(stream).await });
            }
        });
        server
    }

    /// Where the helper connects.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Answers one helper: asks the user when its token was issued and
    /// the job still runs, else replies with no answer.
    async fn serve(&self, stream: impl AsyncRead + AsyncWrite + Unpin) {
        let (read, mut write) = tokio::io::split(stream);
        let mut line = String::new();
        let mut reader = tokio::io::BufReader::new(read.take(MAX_REQUEST));
        let answer = match reader.read_line(&mut line).await {
            Ok(_) => match serde_json::from_str::<Request>(&line) {
                Ok(request) => self.answer(request).await,
                Err(_) => None,
            },
            Err(_) => None,
        };
        let Ok(mut reply) = serde_json::to_string(&Reply { answer }) else {
            return;
        };
        reply.push('\n');
        // A helper gone meanwhile needs no reply.
        let _ = write.write_all(reply.as_bytes()).await;
        let _ = write.shutdown().await;
    }

    async fn answer(&self, request: Request) -> Option<String> {
        let (prompt, revoked) = {
            let tokens = lock(&self.tokens);
            let issued = tokens.get(&request.token)?;
            let prompt = AskpassPrompt {
                prompt_id: self.prompt_ids.fetch_add(1, Ordering::Relaxed),
                kind: PromptKind::of(&request.prompt),
                text: request.prompt,
                workspace_name: issued.workspace_name.clone(),
                repo_names: issued.repo_names.clone(),
                job_label: issued.job_label.clone(),
            };
            (prompt, issued.revoked.clone())
        };
        let mut ended = std::pin::pin!(revoked.notified());
        ended.as_mut().enable();
        // Revoked between the lookup and `enable`: the job has ended.
        if !lock(&self.tokens).contains_key(&request.token) {
            return None;
        }
        tokio::select! {
            answer = self.ui.ask(prompt) => answer,
            () = ended => None,
        }
    }
}

impl AskpassIssuer for AskpassServer {
    fn issue(&self, job: &JobView) -> Option<AskpassGrant> {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let revoked = Arc::new(Notify::new());
        lock(&self.tokens).insert(
            token.clone(),
            Issued {
                workspace_name: job.workspace_name.clone(),
                repo_names: job.repo_names.clone(),
                job_label: job.label.clone(),
                revoked: revoked.clone(),
            },
        );
        let askpass = Askpass {
            program: self.program.clone(),
            env: vec![
                (ADDRESS_VAR.to_owned(), self.address.clone()),
                (TOKEN_VAR.to_owned(), token.clone()),
            ],
        };
        let tokens = self.tokens.clone();
        Some(AskpassGrant::new(askpass, move || {
            lock(&tokens).remove(&token);
            revoked.notify_waiters();
        }))
    }
}

/// Serves this executable as the helper on a new platform socket, asking
/// through `ui`. Runs on the app's async runtime.
pub fn serve(ui: Arc<dyn AskpassUi>) -> std::io::Result<Arc<AskpassServer>> {
    let program = std::env::current_exe()?;
    tauri::async_runtime::block_on(async move {
        let listener = PlatformSockets::listen()?;
        Ok(AskpassServer::start(listener, program, ui))
    })
}

/// The helper mode: when git started this process as its askpass helper,
/// asks the app and returns the exit code; `None` in any other start.
pub fn helper<S: LocalSockets>() -> Option<i32> {
    let address = std::env::var(ADDRESS_VAR).ok()?;
    let token = std::env::var(TOKEN_VAR).unwrap_or_default();
    let prompt = std::env::args().nth(1).unwrap_or_default();
    Some(match ask_app::<S>(&address, token, prompt) {
        Some(answer) => {
            let mut stdout = std::io::stdout().lock();
            let written = writeln!(stdout, "{answer}").and_then(|()| stdout.flush());
            if written.is_ok() { 0 } else { 1 }
        }
        None => 1,
    })
}

/// The app's answer to `prompt`; `None` for no answer or no app.
fn ask_app<S: LocalSockets>(address: &str, token: String, prompt: String) -> Option<String> {
    let mut stream = S::connect(address).ok()?;
    let mut request = serde_json::to_string(&Request { token, prompt }).ok()?;
    request.push('\n');
    stream.write_all(request.as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str::<Reply>(&line).ok()?.answer
}

/// Asks in the app's window: each prompt goes to the UI as event
/// `askpass_prompt` and is answered by command `askpass_answer`; one
/// withdrawn is closed with event `askpass_closed`.
pub struct WindowAskpassUi {
    pending: Mutex<HashMap<u32, oneshot::Sender<Option<String>>>>,
    show: Box<dyn Fn(&AskpassPrompt) + Send + Sync>,
    close: Arc<dyn Fn(u32) + Send + Sync>,
}

impl std::fmt::Debug for WindowAskpassUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowAskpassUi").finish_non_exhaustive()
    }
}

impl WindowAskpassUi {
    /// Shows prompts with `show` and closes withdrawn ones with `close`.
    pub fn new(
        show: impl Fn(&AskpassPrompt) + Send + Sync + 'static,
        close: impl Fn(u32) + Send + Sync + 'static,
    ) -> Self {
        Self {
            pending: Mutex::default(),
            show: Box::new(show),
            close: Arc::new(close),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<u32, oneshot::Sender<Option<String>>>> {
        // Each change is one insert or removal.
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The user's answer to prompt `prompt_id`; `None` is a cancel. Does
    /// nothing when the prompt is no longer open.
    pub fn answer(&self, prompt_id: u32, answer: Option<String>) {
        if let Some(sender) = self.lock().remove(&prompt_id) {
            let _ = sender.send(answer);
        }
    }
}

impl AskpassUi for WindowAskpassUi {
    fn ask(&self, prompt: AskpassPrompt) -> Answer<'_> {
        let id = prompt.prompt_id;
        let (sender, receiver) = oneshot::channel();
        self.lock().insert(id, sender);
        (self.show)(&prompt);
        let open = Open { ui: self, id };
        Box::pin(async move {
            let answer = receiver.await.ok().flatten();
            drop(open);
            answer
        })
    }
}

/// A prompt shown in the window; dropped while still open, it is closed.
struct Open<'a> {
    ui: &'a WindowAskpassUi,
    id: u32,
}

impl Drop for Open<'_> {
    fn drop(&mut self) {
        if self.ui.lock().remove(&self.id).is_some() {
            (self.ui.close)(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_masked_or_yes_no_by_their_text() {
        assert_eq!(
            PromptKind::of("Username for 'https://example.com': "),
            PromptKind::Text
        );
        assert_eq!(
            PromptKind::of("Password for 'https://alice@example.com': "),
            PromptKind::Secret
        );
        assert_eq!(
            PromptKind::of("Enter passphrase for key '/home/a/.ssh/id_ed25519': "),
            PromptKind::Secret
        );
        assert_eq!(
            PromptKind::of("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
            PromptKind::YesNo
        );
    }
}
