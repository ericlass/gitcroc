//! UI-26 at the largest UI font size, 32 px: no label is cut in the
//! toolbar, the tab bar or any dialog, as rc.1's `a11y.spec.ts` checked
//! with `scrollWidth` (M6-25b). GPUI's test platform lays every character
//! out at 0.6 em, wider than most real text, so a control that holds its
//! label here holds it on a desktop.

use gitcroc_core::{Appearance, MAX_FONT_SIZE, Settings};
use gitcroc_ui::strings::clone as clone_text;
use gitcroc_ui::theme::AppTheme;
use gpui_kit::base::test_support::ElementSnapshot;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, Pixels, Role, Size, TestAppContext, VisualTestContext, px, size};

use crate::keyboard::{driver, repo_node};
use crate::palette_ops::Driver;

/// The window's size as the app opens it.
const APP_SIZE: Size<Pixels> = size(px(1280.), px(800.));

/// The roles whose name is the text they show.
const LABELLED: [Role; 5] = [
    Role::Button,
    Role::Tab,
    Role::CheckBox,
    Role::RadioButton,
    Role::MenuItem,
];

/// Controls whose name says more than the text they show: the name, then
/// the text.
const SHOWN: [(&str, &str); 1] = [(clone_text::CHOOSE_FOLDER, clone_text::CHOOSE)];

/// Whether `control` shows only an icon: an icon button is square, and the
/// window's own buttons draw theirs.
fn icon_only(control: &ElementSnapshot) -> bool {
    let size = control.bounds().size;
    size.width == size.height
        || matches!(control.path().last(), Some(ElementId::Name(id)) if id.starts_with("window-"))
}

/// The controls in `scope` whose text is wider than their box, and those
/// that reach past the window's edge. A box two lines high or more wraps
/// its label and grows with it, so its label is cut only by a word wider
/// than the box.
fn cut(cx: &mut VisualTestContext, scope: &str) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        let size = window.rem_size();
        let edge = window.viewport_size().width;
        let style = window.text_style();
        let system = window.text_system().clone();
        let width = |text: &str| {
            system
                .shape_line(
                    text.to_owned().into(),
                    size,
                    &[style.to_run(text.len())],
                    None,
                )
                .width
        };
        let mut found = Vec::new();
        for role in LABELLED {
            for control in window.within(scope.to_owned()).find_all_by_role(role) {
                let Some(label) = control.label().map(str::to_owned) else {
                    continue;
                };
                let bounds = control.bounds();
                if bounds.right() > edge {
                    found.push(format!("{label}: past the window's edge"));
                    continue;
                }
                if icon_only(&control) {
                    continue;
                }
                let shown = SHOWN
                    .iter()
                    .find(|(name, _)| *name == label)
                    .map_or(label.as_str(), |(_, shown)| shown);
                let needed = if bounds.size.height >= size * 2. {
                    shown
                        .split_whitespace()
                        .map(width)
                        .fold(px(0.), Pixels::max)
                } else {
                    width(shown)
                };
                if bounds.size.width < needed {
                    found.push(format!("{label}: {:?} for {needed:?}", bounds.size));
                }
            }
        }
        found
    })
}

/// Opens `dialog` with `open`, notes what is cut in it, and closes it.
#[track_caller]
fn check_dialog(
    d: &mut Driver,
    found: &mut Vec<String>,
    dialog: &str,
    open: impl FnOnce(&mut Driver),
) {
    open(d);
    crate::history::wait_for_element(&mut d.cx, dialog.to_owned().into());
    found.extend(
        cut(&mut d.cx, dialog)
            .into_iter()
            .map(|cut| format!("{dialog}: {cut}")),
    );
    d.key("escape");
}

#[gpui::test]
fn ui_26_no_label_is_cut_at_the_largest_ui_font_size(root: &mut TestAppContext) {
    let mut setup = crate::group_ops::Setup::new(&[
        ("api", crate::group_ops::Remote::Ok),
        ("docs", crate::group_ops::Remote::None),
    ]);
    let api = crate::history::repo(&setup.made, "api");
    api.write("README.md", "changed\n").unwrap();
    setup.add();
    let node = repo_node(&setup.id("api"));
    let f = &setup.fixture;
    let mut d = driver(f, root);
    d.cx.simulate_resize(APP_SIZE);
    let largest = Settings {
        appearance: Appearance {
            ui_font_size: MAX_FONT_SIZE,
            ..Appearance::default()
        },
        ..Settings::default()
    };
    d.cx.update(|_, cx| AppTheme::apply_settings(&largest, cx));
    d.tree_to(&["ungrouped", &node]);

    let mut found = Vec::new();
    for (scope, name) in [("toolbar", "toolbar"), ("title-region", "tab bar")] {
        found.extend(
            cut(&mut d.cx, scope)
                .into_iter()
                .map(|cut| format!("{name}: {cut}")),
        );
    }
    for (dialog, opener) in [
        ("palette", "toolbar-palette"),
        ("settings-dialog", "toolbar-settings"),
        ("branch-dialog", "toolbar-3"),
        ("operation-dialog", "toolbar-4"),
    ] {
        check_dialog(&mut d, &mut found, dialog, |d| d.in_toolbar(opener));
    }
    check_dialog(&mut d, &mut found, "job-list", |d| {
        d.f6_to("status-jobs");
        d.key("enter");
    });
    let delete = if cfg!(target_os = "macos") {
        "backspace"
    } else {
        "delete"
    };
    // At 32 px the file list has no room at the app's size: the file is
    // reached in a larger window, and its dialog measured back at it.
    check_dialog(&mut d, &mut found, "discard-dialog", |d| {
        crate::stage::enlarge(&mut d.cx);
        d.uncommitted(&node);
        d.file_to(gitcroc_ui::changes::List::Unstaged, "README.md");
        d.key(delete);
        d.cx.simulate_resize(APP_SIZE);
    });
    check_dialog(&mut d, &mut found, "name-dialog", |d| {
        d.tree_to(&["ungrouped"]);
        d.menu("New group…");
    });
    check_dialog(&mut d, &mut found, "clone-dialog", |d| d.add_repo("Clone…"));
    let group = setup.group("Backend", &["api", "docs"]);
    let group_node = setup.group_node(&group);
    crate::history::wait_for_element(&mut d.cx, crate::history::tree_row(&group_node));
    for (dialog, opener) in [
        ("preflight", "toolbar-0"),
        ("group-name", "toolbar-3"),
        ("group-sync", "toolbar-options-0"),
    ] {
        check_dialog(&mut d, &mut found, dialog, |d| {
            d.tree_to(&[&group_node]);
            d.in_toolbar(opener);
        });
    }
    check_dialog(&mut d, &mut found, "question", |d| {
        d.tree_to(&[&group_node]);
        d.menu("Delete…");
    });
    assert_eq!(found, Vec::<String>::new());
}
