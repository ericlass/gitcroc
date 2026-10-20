//! PLAT-4, SEC-7, D12: the CI-only updater cannot escape into a release.

use std::{fs, path::PathBuf};

#[test]
fn d12_sec_7_update_test_is_opt_in_and_absent_from_release_builds() {
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(app.join("Cargo.toml")).unwrap()).unwrap();
    assert!(cargo["features"]["update-test"].is_array());
    assert!(!cargo["features"].get("default").is_some_and(|v| {
        v.as_array()
            .unwrap()
            .iter()
            .any(|feature| feature.as_str() == Some("update-test"))
    }));
    for workflow in ["release.yml", "bundles.yml"] {
        let text = fs::read_to_string(app.join("../../.github/workflows").join(workflow)).unwrap();
        assert!(
            !text.contains("update-test"),
            "{workflow} enables the test updater"
        );
    }
}

#[test]
fn plat_4_start_logs_the_overlaid_package_version() {
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).unwrap();
    let start = source.find("log.started(").unwrap();
    assert!(source[start..start + 160].contains("app.package_info().version.to_string()"));
}
