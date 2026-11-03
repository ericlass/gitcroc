//! Drives the real window from a task on GPUI's main thread: waits for
//! what the window draws, presses keys and clicks as a user would, and
//! reads the accessibility tree once AT-SPI has activated it.

use std::time::Duration;

use gpui_kit::base::test_support as observed;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, App, AsyncApp, ElementId, Window};
use serde_json::Value;

/// How long a view may take to show, or the tree to activate. Only reached
/// on a failure.
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);

pub struct Driver {
    pub handle: AnyWindowHandle,
    pub cx: AsyncApp,
}

impl Driver {
    /// Runs `f` on the window.
    pub fn with<R>(&mut self, f: impl FnOnce(&mut Window, &mut App) -> R) -> Result<R, String> {
        self.handle
            .update(&mut self.cx, |_, window, cx| f(window, cx))
            .map_err(|error| format!("the window is gone: {error}"))
    }

    /// Lets the window draw `count` frames.
    pub async fn frames(&mut self, count: usize) -> Result<(), String> {
        for _ in 0..count {
            self.with(|window, _| window.refresh())?;
            self.cx.background_executor().timer(POLL).await;
        }
        Ok(())
    }

    /// Waits until `done` holds for what the window last drew.
    pub async fn until(
        &mut self,
        what: &str,
        mut done: impl FnMut(&mut Window, &mut App) -> bool,
    ) -> Result<(), String> {
        let tries = TIMEOUT.as_millis() / POLL.as_millis();
        for _ in 0..tries {
            self.with(|window, _| window.refresh())?;
            self.cx.background_executor().timer(POLL).await;
            if self.with(&mut done)? {
                return Ok(());
            }
        }
        Err(format!("{what} never showed"))
    }

    /// Waits until element `id` is drawn.
    pub async fn shown(&mut self, id: &str) -> Result<(), String> {
        let element: ElementId = id.to_owned().into();
        self.until(id, |window, _| window.try_find(element.clone()).is_some())
            .await
    }

    /// Clicks element `id` once it is drawn.
    pub async fn click(&mut self, id: &str) -> Result<(), String> {
        self.shown(id).await?;
        let element: ElementId = id.to_owned().into();
        self.with(|window, cx| window.click(element, cx))?;
        self.frames(2).await
    }

    /// The id of the drawn element whose id starts with `prefix` and whose
    /// name starts with `name`.
    pub fn labelled_id(&mut self, prefix: &str, name: &str) -> Result<Option<String>, String> {
        self.with(|window, _| labelled(window, prefix, name))
    }

    /// Waits for the drawn element whose id starts with `prefix` and whose
    /// name starts with `name`. and clicks it.
    pub async fn click_labelled(&mut self, prefix: &str, name: &str) -> Result<(), String> {
        let mut found = None;
        self.until(&format!("{prefix}… named {name}"), |window, _| {
            found = labelled(window, prefix, name);
            found.is_some()
        })
        .await?;
        self.click(&found.unwrap_or_default()).await
    }

    /// Presses `keys`, as "escape" or "ctrl-shift-p".
    pub async fn press(&mut self, keys: &str) -> Result<(), String> {
        self.with(|window, cx| window.press(keys, cx))?;
        self.frames(2).await
    }

    /// Types `text` where the focus is.
    pub async fn input(&mut self, text: &str) -> Result<(), String> {
        self.with(|window, cx| window.input(text, cx))?;
        self.frames(2).await
    }

    /// The accessibility tree of the window as it is drawn now, once AT-SPI
    /// has activated it.
    pub async fn tree(&mut self) -> Result<Value, String> {
        self.until("the accessibility tree", |window, _| {
            window.is_a11y_active()
        })
        .await
        .map_err(|_| {
            "the accessibility tree never activated; is the AT-SPI bus enabled?".to_owned()
        })?;
        // A frame after whatever the last step changed, and one more so
        // that its tree update is the last one.
        self.frames(3).await?;
        let json = self
            .with(|window, _| window.debug_a11y_tree_json())?
            .ok_or("the window has drawn no accessibility tree")?;
        serde_json::from_str(&json).map_err(|error| format!("the tree is not JSON: {error}"))
    }
}

/// The id of the drawn element whose id starts with `prefix` and whose name
/// starts with `name`.
fn labelled(window: &Window, prefix: &str, name: &str) -> Option<String> {
    observed::snapshots(window).iter().find_map(|found| {
        let ElementId::Name(id) = found.path().last()? else {
            return None;
        };
        (id.starts_with(prefix) && found.label()?.starts_with(name)).then(|| id.to_string())
    })
}
