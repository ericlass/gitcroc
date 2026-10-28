//! The main window: its state from the app's events (ARCH-6), its calls off
//! the UI thread (ARCH-7), and its regions (UI-22).

use std::sync::mpsc;
use std::thread::ThreadId;
use std::time::Duration;

use gitcroc_app::app::AppEvent;
use gitcroc_core::GitStatus;
use gitcroc_ui::services::Services;
use gitcroc_ui::window::GitcrocWindow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, IntoElement, Keystroke, Render, TestAppContext, VisualTestContext, Window,
    div,
};
use tokio::sync::broadcast::error::RecvError;

use crate::support::{Fixture, main_window, open, settle};
use gitcroc_ui::keys::KEYMAP;

fn title(window: &gpui_kit::Entity<GitcrocWindow>, cx: &gpui_kit::App) -> String {
    let title_bar = window.read(cx).title_bar().clone();
    title_bar.read(cx).title().to_string()
}

#[gpui::test]
fn arch_6_window_reads_state_again_after_lag(cx: &mut TestAppContext) {
    // A channel of one event: the window, whose task does not run until
    // the test lets it, falls behind at the second.
    let fixture = Fixture::start(1);
    let (mut cx, window) = main_window(cx, &fixture);
    let first = cx.update(|_, cx| title(&window, cx));

    let app = fixture.app().clone();
    let mut events = app.subscribe();
    fixture.runtime.block_on(async {
        app.workspace_create("Latest".to_owned()).await.unwrap();
        // Git detection's events come after the workspaces' one, so the
        // channel no longer holds it.
        app.git_recheck();
        loop {
            match events.recv().await {
                Ok(AppEvent::GitStatus(GitStatus::Checking)) | Err(RecvError::Lagged(_)) => {}
                Ok(AppEvent::GitStatus(_)) => break,
                Ok(_) => {}
                Err(RecvError::Closed) => panic!("the app stopped"),
            }
        }
    });
    assert_ne!(first, "Latest");

    settle(&mut cx, |_, cx| title(&window, cx) == "Latest");
    cx.update(|_, cx| {
        let tabs = window.read(cx).tabs(cx);
        assert_eq!(tabs, app.workspace_get());
    });
}

/// A view that calls a service, and counts the frames it renders.
struct Caller {
    services: Services,
    frames: usize,
    ran_on: Option<ThreadId>,
}

impl Caller {
    /// Calls a service that blocks until `release` is sent, and keeps the
    /// thread it ran on.
    fn call(&mut self, release: mpsc::Receiver<()>, cx: &mut Context<Self>) {
        let call = self.services.call(async move {
            // The test sends it once it has rendered frames; the timeout
            // only ends a test that already failed.
            let released = release.recv_timeout(Duration::from_secs(10)).is_ok();
            released.then(|| std::thread::current().id())
        });
        cx.spawn(async move |this, cx| {
            let ran_on = call.await.flatten();
            let _ = this.update(cx, |this, cx| {
                this.ran_on = ran_on;
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for Caller {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        div()
    }
}

#[gpui::test]
fn arch_7_service_calls_do_not_run_on_the_ui_thread(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let services = fixture.services.clone();
    let (mut cx, caller) = open(cx, move |_, cx| {
        cx.new(|_| Caller {
            services,
            frames: 0,
            ran_on: None,
        })
    });
    let ui_thread = std::thread::current().id();

    let (release, released) = mpsc::channel();
    cx.update(|_, cx| caller.update(cx, |caller, cx| caller.call(released, cx)));
    // While the call blocks, the window goes on rendering.
    let before = cx.update(|_, cx| caller.read(cx).frames);
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.refresh();
            window.render_frame(cx);
        });
    }
    cx.update(|_, cx| {
        let caller = caller.read(cx);
        assert!(
            caller.frames >= before + 3,
            "{} frames",
            caller.frames - before
        );
        assert_eq!(caller.ran_on, None);
    });

    release.send(()).unwrap();
    settle(&mut cx, |_, cx| caller.read(cx).ran_on.is_some());
    cx.update(|_, cx| assert_ne!(caller.read(cx).ran_on, Some(ui_thread)));
}

#[gpui::test]
fn ui_22_every_shortcut_is_bound(cx: &mut TestAppContext) {
    cx.update(gitcroc_ui::init);
    let keymap = cx.update(|cx| cx.key_bindings());
    let keymap = keymap.borrow();
    for (shortcut, keys) in KEYMAP {
        let action = shortcut.action();
        let bound: Vec<&Keystroke> = keymap
            .bindings_for_action(action.as_ref())
            .flat_map(|binding| binding.keystrokes())
            .map(|keystroke| keystroke.inner())
            .collect();
        let [bound] = bound.as_slice() else {
            panic!("{shortcut:?} is bound to {bound:?}");
        };
        let modifiers = bound.modifiers;
        let secondary = if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.control
        };
        assert_eq!(
            (
                bound.key.as_str(),
                secondary,
                modifiers.shift,
                modifiers.alt
            ),
            (keys.key, keys.secondary, keys.shift, keys.alt),
            "{shortcut:?}"
        );
    }
}

#[gpui::test]
fn ui_22_f6_moves_the_focus_between_regions(cx: &mut TestAppContext) {
    let fixture = Fixture::start(gitcroc_app::app::EVENT_CAPACITY);
    let (mut cx, _) = main_window(cx, &fixture);
    let regions = [
        "title-region",
        "toolbar",
        "tree-list",
        "history",
        "changes",
        "status-bar",
    ];
    let focused = |cx: &mut VisualTestContext| -> Vec<&str> {
        cx.update(|window, cx| {
            window.render_frame(cx);
            regions
                .into_iter()
                .filter(|region| window.find(*region).focused() == Some(true))
                .collect()
        })
    };

    // The tree has the focus when the window opens.
    assert_eq!(focused(&mut cx), ["tree-list"]);
    for region in regions.iter().cycle().skip(3).take(6) {
        cx.update(|window, cx| window.press("f6", cx));
        assert_eq!(focused(&mut cx), [*region]);
    }
    for region in ["toolbar", "title-region", "status-bar"] {
        cx.update(|window, cx| window.press("shift-f6", cx));
        assert_eq!(focused(&mut cx), [region]);
    }
}
