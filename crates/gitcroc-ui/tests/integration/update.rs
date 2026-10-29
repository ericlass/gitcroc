//! The update notice and dialog (PLAT-4): a fake source offers a version,
//! and the test chooses the package and lets the app check, which a debug
//! build never does otherwise (D12). The update itself is M6-27's.

use std::sync::Arc;

use gitcroc_app::update::UpdateTest;
use gitcroc_core::{Package, UpdateFound, UpdateFuture, UpdateProgress, UpdateSource};
use gitcroc_ui::strings;
use gpui_kit::{TestAppContext, VisualTestContext};

use crate::history::{click, wait_until};
use crate::jobs::label_of;
use crate::support::{Fixture, Options, main_window};

const VERSION: &str = "9.9.9";

/// Offers `VERSION` with `notes`; installing it succeeds.
struct Offer {
    notes: String,
}

impl UpdateSource for Offer {
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
        let found = UpdateFound {
            version: VERSION.to_owned(),
            date: Some("2026-10-01T12:00:00Z".to_owned()),
            notes: self.notes.clone(),
        };
        Box::pin(async move { Ok(Some(found)) })
    }

    fn install(&self, _progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

/// An app from `package` that is offered `VERSION` with `notes`, and a
/// window showing the notice.
fn offered(cx: &mut TestAppContext, package: Package, notes: &str) -> (Fixture, VisualTestContext) {
    let fixture = Fixture::start_with(Options {
        updates: Some((
            Arc::new(Offer {
                notes: notes.to_owned(),
            }),
            UpdateTest {
                package,
                checks: true,
            },
        )),
        ..Options::default()
    });
    let (mut cx, _) = main_window(cx, &fixture);
    fixture.app().update_check_now();
    let notice = strings::update::notice(VERSION);
    wait_until(&mut cx, |cx| {
        label_of(cx, "update-notice").as_deref() == Some(notice.as_str())
    });
    (fixture, cx)
}

/// PLAT-4: the notice names the version and opens the dialog, which shows
/// both versions and the notes; Later hides the notice.
#[gpui::test]
fn plat_4_update_notice_opens_the_dialog(cx: &mut TestAppContext) {
    let (fixture, mut cx) = offered(cx, Package::AppImage, "Faster.\nSmaller.");
    click(&mut cx, "update-notice".into());
    let current = fixture.app().update_status().current_version;
    assert_eq!(
        label_of(&mut cx, "update-versions"),
        Some(strings::update::versions(&current, VERSION))
    );
    assert_eq!(
        label_of(&mut cx, "update-notes").as_deref(),
        Some("Faster.\nSmaller.")
    );
    assert!(label_of(&mut cx, "update-published").is_some());

    click(&mut cx, "update-later".into());
    assert_eq!(label_of(&mut cx, "update-dialog"), None);
    wait_until(&mut cx, |cx| label_of(cx, "update-notice").is_none());
}

/// PLAT-4: a package that installs itself has Install and restart; once
/// the update is installed, the app restarts.
#[gpui::test]
fn plat_4_install_and_restart(cx: &mut TestAppContext) {
    let mut restarted = cx.expect_restart();
    let (_fixture, mut cx) = offered(cx, Package::AppImage, "");
    click(&mut cx, "update-notice".into());
    assert_eq!(label_of(&mut cx, "update-download"), None);
    assert_eq!(
        label_of(&mut cx, "update-notes").as_deref(),
        Some(strings::update::NO_NOTES)
    );
    click(&mut cx, "update-install".into());
    wait_until(&mut cx, |_| matches!(restarted.try_recv(), Ok(Some(_))));
}

/// PLAT-4: a `.deb` only notifies: Open download page, and no Install.
#[gpui::test]
fn plat_4_notify_only_packages_offer_the_download_page(cx: &mut TestAppContext) {
    let (_fixture, mut cx) = offered(cx, Package::Deb, "Notes");
    click(&mut cx, "update-notice".into());
    assert_eq!(label_of(&mut cx, "update-install"), None);
    assert_eq!(
        label_of(&mut cx, "update-download").as_deref(),
        Some(strings::update::OPEN_DOWNLOAD_PAGE)
    );
    assert_eq!(
        label_of(&mut cx, "update-notify-only").as_deref(),
        Some(strings::update::NOTIFY_ONLY)
    );
}

/// SEC-3: release notes show as written, never as markup.
#[gpui::test]
fn sec_3_release_notes_render_as_text(cx: &mut TestAppContext) {
    let notes = "<b>Bold</b> <script>x</script>";
    let (_fixture, mut cx) = offered(cx, Package::AppImage, notes);
    click(&mut cx, "update-notice".into());
    assert_eq!(label_of(&mut cx, "update-notes").as_deref(), Some(notes));
}
