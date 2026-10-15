//! How the window starts (M1-15): created hidden and shown once its page
//! starts loading. Shown at creation, WebKitGTK holds the page's start back
//! by up to 500 ms on some GPUs, which `just bench-ui` measures as cold start.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn perf_3_the_window_is_shown_once_its_page_loads() {
    let path = app_dir().join("tauri.conf.json");
    let config: Value = serde_json::from_str(&read(&path))
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let windows = config["app"]["windows"].as_array().expect("app.windows");
    assert!(!windows.is_empty(), "no window is configured");
    for window in windows {
        assert_eq!(
            window["visible"],
            Value::Bool(false),
            "window {} must start hidden",
            window["label"]
        );
    }

    // Shown from the page load handler, and from nowhere else at startup.
    let source = read(&app_dir().join("src/lib.rs"));
    let handler = source
        .split(".on_page_load(")
        .nth(1)
        .and_then(|rest| rest.split(".setup(").next())
        .expect("run() registers a page load handler before setup");
    assert!(
        handler.contains(".show()"),
        "the page load handler shows the window"
    );
    assert_eq!(
        source.matches(".show()").count(),
        1,
        "the window is shown only on page load"
    );
}
