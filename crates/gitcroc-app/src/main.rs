// No console window behind the app in Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), gitcroc_app::Error> {
    gitcroc_app::run()
}
