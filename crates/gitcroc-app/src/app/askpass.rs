//! The askpass dialog's answers (D10).

use super::App;

impl App {
    /// The user's answer to prompt `prompt_id` of an `AskpassPrompt`, which
    /// goes to git and nowhere else (D10); `None` is a cancel, and git gets
    /// no answer. A prompt no longer open is ignored.
    pub fn askpass_answer(&self, prompt_id: u32, answer: Option<String>) {
        self.inner.askpass.answer(prompt_id, answer);
    }
}
