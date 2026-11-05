//! QA-3, D12: the smoke-only interface never enters a production build.
#[test]
fn qa_3_smoke_feature_is_explicit_and_never_packaged() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().unwrap();
    assert!(manifest["features"].get("e2e").is_some());
    assert!(manifest["features"].get("default").is_none());
    for workflow in [
        include_str!("../../../.github/workflows/packages.yml"),
        include_str!("../../../.github/workflows/release.yml"),
    ] {
        assert!(!workflow.contains("--features e2e"));
    }
}

#[test]
fn sec_2_native_assets_are_embedded_and_external_locations_are_refused() {
    use gitcroc_ui::icons::{CHECK, Icons};
    use gpui_kit::AssetSource;
    assert_eq!(
        Icons.load(CHECK).unwrap().unwrap().as_ref(),
        include_bytes!("../assets/icons/check.svg")
    );
    for path in [
        "https://example.com/page.svg",
        "file:///tmp/page.svg",
        "../../page.svg",
    ] {
        assert!(Icons.load(path).unwrap().is_none());
        assert!(Icons.list(path).unwrap().is_empty());
    }
}
