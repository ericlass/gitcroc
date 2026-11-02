//! What an export or an import of a workspace said (DATA-2), a port of
//! rc.1's `ui/src/stores/share.ts`: the core opens the save or open dialog
//! (SEC-2), writes or reads the file and reports how it went; this keeps
//! the notice the window shows, as text (SEC-3), until it is dismissed or
//! the next export or import starts. An import with missing repos asks for
//! the missing repos dialog.

use gitcroc_app::app::AppEvent;
use gitcroc_core::WorkspaceId;
use gpui_kit::SharedString;

use crate::strings::share as text;

/// The last export's or import's notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareNotice {
    pub text: SharedString,
    /// It says why something failed, or that paths stay absolute.
    pub error: bool,
}

/// What the window does after an export or import event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Heard {
    /// The notice to show, replacing the one before.
    pub notice: Option<ShareNotice>,
    /// The imported workspace whose missing repos the dialog lists.
    pub missing: Option<WorkspaceId>,
}

/// `text` with a capital first letter, as the core's messages start in
/// lower case.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// What the window shows for `event`, when it is an export's or an
/// import's end; `None` for any other event.
pub fn hear(event: &AppEvent) -> Option<Heard> {
    match event {
        AppEvent::WorkspaceExported { report, error, .. } => {
            let notice = match report {
                Some(report) => ShareNotice {
                    text: text::exported(&report.path, &report.absolute).into(),
                    error: !report.absolute.is_empty(),
                },
                None => ShareNotice {
                    text: sentence(error.as_deref().unwrap_or(text::EXPORT_FAILED)).into(),
                    error: true,
                },
            };
            Some(Heard {
                notice: Some(notice),
                missing: None,
            })
        }
        AppEvent::WorkspaceImported { report, error } => Some(match report {
            Some(report) => Heard {
                notice: None,
                missing: (!report.missing.is_empty()).then(|| report.workspace_id.clone()),
            },
            None => Heard {
                notice: Some(ShareNotice {
                    text: sentence(error.as_deref().unwrap_or(text::IMPORT_FAILED)).into(),
                    error: true,
                }),
                missing: None,
            },
        }),
        _ => None,
    }
}
