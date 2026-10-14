// No console window behind the app in Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), gitcroc_app::Error> {
    // Started by git as its askpass helper (D10): no window.
    if let Some(code) = gitcroc_app::askpass::helper::<gitcroc_app::platform::PlatformSockets>() {
        std::process::exit(code);
    }
    gitcroc_app::run()
}
