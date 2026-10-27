//! The system's accent colour (UI-23), where an API without `unsafe` gives
//! it (QA-4): Windows' `UISettings` and the Linux appearance portal. macOS
//! keeps the tokens' accent. Each change reaches [`AppTheme`] on the main
//! thread.

use gpui_kit::App;

use super::AppTheme;
use super::tokens::Color;

/// Follows the system's accent colour until the app ends. Not called in
/// tests: it talks to the desktop.
pub fn follow(cx: &mut App) {
    let (sender, mut received) = tokio::sync::mpsc::unbounded_channel::<Option<Color>>();
    platform::listen(sender, cx);
    cx.spawn(async move |cx| {
        while let Some(accent) = received.recv().await {
            cx.update(|cx| AppTheme::set_system_accent(accent, cx));
        }
    })
    .detach();
}

/// A colour from channels of 0 to 1; `None` when one is out of range, which
/// the portal uses for "no accent".
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn from_unit(red: f64, green: f64, blue: f64) -> Option<Color> {
    let channel = |value: f64| {
        (0. ..=1.)
            .contains(&value)
            .then(|| (value * 255.).round() as u8)
    };
    Some(Color::from_channels([
        channel(red)?,
        channel(green)?,
        channel(blue)?,
    ]))
}

#[cfg(target_os = "linux")]
mod platform {
    use ashpd::desktop::settings::Settings;
    use futures_lite::StreamExt;
    use gpui_kit::App;
    use tokio::sync::mpsc::UnboundedSender;

    use super::{Color, from_unit};

    /// The portal's accent, then each change. Without a portal, or with one
    /// that has no accent, nothing is sent and the tokens' accent stays.
    pub fn listen(sender: UnboundedSender<Option<Color>>, cx: &mut App) {
        cx.background_executor()
            .spawn(async move {
                let Ok(settings) = Settings::new().await else {
                    return;
                };
                if let Ok(color) = settings.accent_color().await {
                    let _ = sender.send(from_unit(color.red(), color.green(), color.blue()));
                }
                let Ok(mut changes) = settings.receive_accent_color_changed().await else {
                    return;
                };
                while let Some(color) = changes.next().await {
                    if sender
                        .send(from_unit(color.red(), color.green(), color.blue()))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use gpui_kit::App;
    use tokio::sync::mpsc::UnboundedSender;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    use windows::core::IInspectable;

    use super::Color;

    fn accent(settings: &UISettings) -> Option<Color> {
        let color = settings.GetColorValue(UIColorType::Accent).ok()?;
        Some(Color::from_channels([color.R, color.G, color.B]))
    }

    /// `UISettings`' accent, then each change, which Windows reports on a
    /// thread of its own.
    pub fn listen(sender: UnboundedSender<Option<Color>>, cx: &mut App) {
        let Ok(settings) = UISettings::new() else {
            return;
        };
        let _ = sender.send(accent(&settings));
        let handler = TypedEventHandler::<UISettings, IInspectable>::new(move |settings, _| {
            if let Some(settings) = settings.as_ref() {
                let _ = sender.send(accent(settings));
            }
            Ok(())
        });
        if let Ok(token) = settings.ColorValuesChanged(&handler) {
            // The settings object must live for its events to come.
            cx.set_global(Subscription { settings, token });
        }
    }

    struct Subscription {
        settings: UISettings,
        token: i64,
    }

    impl gpui_kit::Global for Subscription {}

    impl Drop for Subscription {
        fn drop(&mut self) {
            let _ = self.settings.RemoveColorValuesChanged(self.token);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    use gpui_kit::App;
    use tokio::sync::mpsc::UnboundedSender;

    use super::Color;

    /// macOS gives its accent only through APIs that need `unsafe` (QA-4).
    pub fn listen(_: UnboundedSender<Option<Color>>, _: &mut App) {}
}
