//! The window's one polite live status (M5-10), a port of
//! `ui/src/stores/announcer.ts`: what a screen reader says without the
//! focus moving. A job of any tab that ended failed or in conflict is
//! announced here, and so is a newer version's notice (PLAT-4). Progress
//! is never announced, so a running job does not talk on every step.

use gitcroc_core::{JobView, Outcome};
use gpui_kit::accesskit::Live;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    InteractiveElement, IntoElement, Role, SharedString, StatefulInteractiveElement, Styled,
    TestSupportExt, div,
};

use crate::strings;

/// What the live status says.
#[derive(Debug, Default)]
pub struct Announcer {
    text: SharedString,
    /// The version whose notice was said, so the daily check finding it
    /// again says nothing.
    announced_version: Option<String>,
}

impl Announcer {
    /// What the live status holds: empty until the first announcement, so
    /// nothing is said when the window opens.
    pub fn text(&self) -> &SharedString {
        &self.text
    }

    /// Says `message`. The same message again is said again: a no-break
    /// space makes it a change.
    pub fn announce(&mut self, message: String) {
        self.text = if self.text.as_ref() == message {
            format!("{message}\u{a0}")
        } else {
            message
        }
        .into();
    }

    /// Says how `job` ended when it failed or stopped on a conflict;
    /// returns whether it said anything.
    pub fn job_ended(&mut self, job: &JobView) -> bool {
        match &job.outcome {
            Some(outcome @ (Outcome::Failed | Outcome::Conflict)) => {
                self.announce(strings::jobs::announced(
                    &job.label,
                    &job.workspace_name,
                    &strings::jobs::outcome(outcome),
                ));
                true
            }
            _ => false,
        }
    }

    /// Says the notice of `version` as it first appears; returns whether it
    /// said anything.
    pub fn update_notice(&mut self, version: Option<&str>) -> bool {
        let Some(version) = version else {
            return false;
        };
        if self.announced_version.as_deref() == Some(version) {
            return false;
        }
        self.announced_version = Some(version.to_owned());
        self.announce(strings::update::notice(version));
        true
    }
}

/// The live status node: polite, and drawn at no size, since what it says
/// is on screen already, in the status bar, the tab's marker or the
/// notice. It has no name until it has something to say: AccessKit
/// announces a live node's name, an empty one too, when the node appears.
/// GPUI has no `aria_live`; synthetic children are the one place that can
/// set it on the node.
pub fn live_status(text: &SharedString) -> impl IntoElement + use<> {
    div()
        .id("live-status")
        .test_support()
        .role(Role::Status)
        .when(!text.is_empty(), |node| node.aria_label(text.clone()))
        .a11y_synthetic_children(|builder| builder.parent_node().set_live(Live::Polite))
        .absolute()
        .size_0()
        .overflow_hidden()
}

#[cfg(test)]
mod tests {
    use gitcroc_core::{JobId, JobState, WorkspaceId};

    use super::*;

    fn ended(outcome: Outcome) -> JobView {
        JobView {
            job_id: JobId(1),
            workspace_id: WorkspaceId("w".into()),
            workspace_name: "Work".into(),
            label: "Fetch".into(),
            repos: Vec::new(),
            repo_names: Vec::new(),
            state: JobState::Ended,
            done: 0,
            total: 0,
            step: None,
            started_at: None,
            duration_ms: None,
            outcome: Some(outcome),
            output: String::new(),
            message: None,
            sub_jobs: Vec::new(),
            report: None,
            ai_files: Vec::new(),
        }
    }

    #[test]
    fn ui_21_only_a_failed_or_conflicted_job_is_announced() {
        let mut said = Announcer::default();
        assert_eq!(said.text().as_ref(), "");
        assert!(!said.job_ended(&ended(Outcome::Succeeded)));
        assert!(!said.job_ended(&ended(Outcome::NothingToDo)));
        assert_eq!(said.text().as_ref(), "");
        assert!(said.job_ended(&ended(Outcome::Failed)));
        assert_eq!(said.text().as_ref(), "Fetch in Work: Failed");
        assert!(said.job_ended(&ended(Outcome::Conflict)));
        assert_eq!(said.text().as_ref(), "Fetch in Work: Conflict");
    }

    #[test]
    fn ui_21_the_same_failure_again_is_said_again() {
        let mut said = Announcer::default();
        said.job_ended(&ended(Outcome::Failed));
        let first = said.text().clone();
        said.job_ended(&ended(Outcome::Failed));
        assert_ne!(said.text(), &first);
        assert_eq!(said.text().trim_end_matches('\u{a0}'), first.as_ref());
    }

    #[test]
    fn plat_4_a_new_version_is_said_once() {
        let mut said = Announcer::default();
        assert!(!said.update_notice(None));
        assert!(said.update_notice(Some("1.1.0")));
        assert_eq!(said.text().as_ref(), "gitcroc 1.1.0 is available");
        assert!(!said.update_notice(Some("1.1.0")));
        assert!(said.update_notice(Some("1.2.0")));
        assert_eq!(said.text().as_ref(), "gitcroc 1.2.0 is available");
    }
}
