//! QA-3: a smoke script drives the real window; compiled only with `e2e`.
//! Test directories and picked folders are explicit, never a user's files.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gitcroc_app::platform::{DialogFuture, Dialogs, FileFilter};
use gitcroc_core::{CacheDir, ConfigDir, LogDir};
use gpui_kit::base::test_support as observed;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, App, ElementId};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub home: PathBuf,
    pub folder: PathBuf,
    pub steps: Vec<Step>,
    /// Leave the isolated scene open for documentation screenshots.
    #[serde(default)]
    pub hold: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Step {
    JobEnded { label: String },
    Click { id: String },
    ClickLabel { prefix: String, label: String },
    Input { text: String },
    Press { keys: String },
    Shown { id: String },
    Gone { id: String },
    Label { prefix: String, label: String },
}

pub fn read(path: &Path) -> Result<Script, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

impl ConfigDir for Script {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.home.join("config"))
    }
}
impl CacheDir for Script {
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.home.join("cache"))
    }
}
impl LogDir for Script {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.home.join("logs"))
    }
}

pub struct PickFolder(pub PathBuf);
impl Dialogs for PickFolder {
    fn pick_folder(&self, _: &str) -> DialogFuture {
        let path = self.0.clone();
        Box::pin(async move { Some(path) })
    }
    fn pick_file(&self, _: &str, _: Option<&FileFilter>) -> DialogFuture {
        Box::pin(async { None })
    }
    fn save_file(&self, _: &str, _: &str, _: Option<&FileFilter>) -> DialogFuture {
        Box::pin(async { None })
    }
}

/// Uses the element registry of the frame just drawn, then lets the next
/// frame present the result. A deadline ends a hung step, never advances it.
pub fn start(handle: AnyWindowHandle, script: Script, app: gitcroc_app::app::App, cx: &mut App) {
    cx.spawn(async move |cx| {
        let result = async {
            for step in script.steps {
                eprintln!("smoke: {step:?}");
                let deadline = Instant::now() + Duration::from_secs(60);
                loop {
                    let done = handle
                        .update(cx, |_, window, cx| {
                            window.refresh();
                            let labelled = |prefix: &str, label: &str| {
                                observed::snapshots(window).iter().find_map(|entry| {
                                    let id = entry.path().last()?;
                                    let name = match id {
                                        ElementId::Name(name)
                                        | ElementId::NamedInteger(name, _) => name.as_ref(),
                                        _ => return None,
                                    };
                                    (name.starts_with(prefix)
                                        && entry.disabled() != Some(true)
                                        && entry
                                            .label()
                                            .is_some_and(|text| text.starts_with(label)))
                                    .then(|| id.clone())
                                })
                            };
                            match &step {
                                Step::JobEnded { label } => app.jobs_get().iter().any(|job| {
                                    job.label.starts_with(label)
                                        && job.state == gitcroc_core::JobState::Ended
                                }),
                                Step::Click { id } => {
                                    let element: ElementId = id.clone().into();
                                    if window
                                        .try_find(element.clone())
                                        .is_some_and(|found| found.disabled() != Some(true))
                                    {
                                        window.click(element, cx);
                                        true
                                    } else {
                                        false
                                    }
                                }
                                Step::ClickLabel { prefix, label } => {
                                    if let Some(id) = labelled(prefix, label) {
                                        window.click(id, cx);
                                        true
                                    } else {
                                        false
                                    }
                                }
                                Step::Input { text } => {
                                    window.input(text, cx);
                                    true
                                }
                                Step::Press { keys } => {
                                    window.press(keys, cx);
                                    true
                                }
                                Step::Shown { id } => window
                                    .try_find(ElementId::Name(id.clone().into()))
                                    .is_some(),
                                Step::Gone { id } => window
                                    .try_find(ElementId::Name(id.clone().into()))
                                    .is_none(),
                                Step::Label { prefix, label } => labelled(prefix, label).is_some(),
                            }
                        })
                        .map_err(|error| error.to_string())?;
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                    if done {
                        break;
                    }
                    if Instant::now() >= deadline {
                        let elements = handle
                            .update(cx, |_, window, _| {
                                format!("{:#?}", observed::snapshots(window))
                            })
                            .unwrap_or_default();
                        return Err(format!("{step:?} timed out; elements: {elements}"));
                    }
                }
            }
            Ok::<_, String>(())
        }
        .await;
        if result.is_ok() && script.hold {
            eprintln!("smoke: ready for capture");
            return;
        }
        if let Err(error) = result {
            eprintln!("smoke failed: {error}");
            crate::BENCH_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qa_3_smoke_rejects_unknown_actions_and_fields() {
        assert!(serde_json::from_str::<Step>(r#"{"action":"shell","command":"git"}"#).is_err());
        assert!(
            serde_json::from_str::<Step>(r#"{"action":"click","id":"toolbar","typo":true}"#)
                .is_err()
        );
        assert!(matches!(
            serde_json::from_str::<Step>(r#"{"action":"click","id":"toolbar"}"#).unwrap(),
            Step::Click { .. }
        ));
    }
}
