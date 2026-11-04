//! Reduced motion (M6-25b). gpui-base sets [`App::reduce_motion`] from
//! macOS, Windows and the `reduced-motion` key of the XDG appearance
//! portal, and GPUI's animations stand still under it: a spinner keeps its
//! first frame and its name (UI-3), and progress, which is never animated,
//! still counts up. Where the portal has no such key, as on Cinnamon, the
//! app reads GNOME's animation switch once at start.

use gpui_kit::App;

/// Reads the Linux fallback off the UI thread (ARCH-7) and applies it. Not
/// called in tests: it asks the desktop.
pub fn follow(cx: &mut App) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let reading = cx.background_executor().spawn(async { read() });
    cx.spawn(async move |cx| {
        if reading.await == Some(false) {
            cx.update(|cx| {
                if !cx.reduce_motion() {
                    cx.set_reduce_motion(true);
                    cx.refresh_windows();
                }
            });
        }
    })
    .detach();
}

/// Whether GNOME's `enable-animations` is on, by `gsettings`, with an
/// argument array (ARCH-3); `None` where there is no such key or no
/// `gsettings`.
fn read() -> Option<bool> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    animations_enabled(&String::from_utf8_lossy(&output.stdout))
}

/// What `gsettings get` printed for a boolean key.
fn animations_enabled(printed: &str) -> Option<bool> {
    match printed.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::animations_enabled;

    #[test]
    fn ui_23_the_linux_fallback_reads_gnomes_animation_switch() {
        assert_eq!(animations_enabled("true\n"), Some(true));
        assert_eq!(animations_enabled("false\n"), Some(false));
        // No such schema or key: gsettings prints its complaint instead.
        assert_eq!(animations_enabled(""), None);
        assert_eq!(
            animations_enabled("No such key “enable-animations”\n"),
            None
        );
    }
}
