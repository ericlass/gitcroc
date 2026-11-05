//! UI-22, PERF-1: wheel scrolling also has a visible, draggable control.
#[test]
fn ui_22_scroll_areas_do_not_omit_scrollbar_widgets() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    fn scan(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                if path.file_name().is_some_and(|name| name == "scrollbars.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                assert!(
                    !source.contains(".overflow_y_scroll()")
                        || source.contains(".vertical_scrollbar(&self.output_scroll)"),
                    "{} enables scrolling without a scrollbar",
                    path.display()
                );
            }
        }
    }
    scan(&root);
}
