//! The askpass dialog's answers (D10), and the prompts' end when the app
//! quits.

use std::time::Duration;

use super::App;

/// The longest [`App::shutdown`] waits for the helpers' replies: a local
/// process that connected and never asked holds the quit no longer.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(2);

impl App {
    /// The user's answer to prompt `prompt_id` of an `AskpassPrompt`, which
    /// goes to git and nowhere else (D10); `None` is a cancel, and git gets
    /// no answer. A prompt no longer open is ignored.
    pub fn askpass_answer(&self, prompt_id: u32, answer: Option<String>) {
        self.inner.askpass.answer(prompt_id, answer);
    }

    /// Ends what would outlive the app, before its runtime goes; the
    /// binary calls it as it quits. Every askpass prompt still open gets no
    /// answer, as after Cancel, so git fails and its helper exits, and each
    /// dialog gets `AskpassClosed`; git asks no more (D10). Jobs still
    /// running are then abandoned with the runtime.
    pub async fn shutdown(&self) {
        if let Some(server) = &self.inner.askpass_server {
            // Past the limit, the helpers left fail when the app's end of
            // the socket closes.
            let _ = tokio::time::timeout(SHUTDOWN_LIMIT, server.close()).await;
        }
    }
}
