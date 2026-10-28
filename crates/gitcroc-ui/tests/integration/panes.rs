//! The three panes' layout (UI-19, UI-29): the changes pane docked right
//! or at the bottom, kept per workspace, and every pane kept in the window
//! as it gets smaller, as rc.1's `ui/e2e/layout.spec.ts` checks.

use gitcroc_app::app::EVENT_CAPACITY;
use gitcroc_core::{Dock, PaneLayout, WorkspaceId};
use gitcroc_ui::panes::{MIN_PANE_HEIGHT, MIN_PANE_WIDTH};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Bounds, Pixels, TestAppContext, VisualTestContext, point, px, size};

use crate::history::{click, tab, wait_until};
use crate::support::{Fixture, main_window};

const DOCK: &str = "changes-dock";
const TREE_SPLITTER: &str = "pane-splitter-tree";
const HISTORY_SPLITTER: &str = "pane-splitter-history";
const CHANGES_SPLITTER: &str = "pane-splitter-changes";

fn bounds(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).bounds()
    })
}

fn width(cx: &mut VisualTestContext, id: &'static str) -> f32 {
    f32::from(bounds(cx, id).size.width)
}

fn height(cx: &mut VisualTestContext, id: &'static str) -> f32 {
    f32::from(bounds(cx, id).size.height)
}

fn label(cx: &mut VisualTestContext, id: &'static str) -> Option<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

fn docked_at_bottom(cx: &mut VisualTestContext) -> bool {
    let (history, changes) = (bounds(cx, "history"), bounds(cx, "changes"));
    changes.origin.y > history.origin.y && changes.origin.x == history.origin.x
}

fn docked_right(cx: &mut VisualTestContext) -> bool {
    let (history, changes) = (bounds(cx, "history"), bounds(cx, "changes"));
    changes.origin.x > history.origin.x && changes.origin.y == history.origin.y
}

/// Drags splitter `id` by `by` pixels: right, or down when `vertical`.
fn drag(cx: &mut VisualTestContext, id: &'static str, by: f32, vertical: bool) {
    let from = bounds(cx, id).center();
    let to = from
        + if vertical {
            point(px(0.), px(by))
        } else {
            point(px(by), px(0.))
        };
    cx.update(|window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.update(|window, cx| window.press(keys, cx));
    cx.run_until_parked();
}

fn resize(cx: &mut VisualTestContext, width: f32, height: f32) {
    cx.simulate_resize(size(px(width), px(height)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

/// Every pane lies inside the window, at least its minimum wide and high.
#[track_caller]
fn assert_in_window(
    cx: &mut VisualTestContext,
    panes: &[&'static str],
    min_width: f32,
    min_height: f32,
) {
    let window = cx.update(|window, _| Bounds::new(point(px(0.), px(0.)), window.viewport_size()));
    for pane in panes {
        let pane_bounds = bounds(cx, pane);
        assert!(
            pane_bounds.right() <= window.right() && pane_bounds.bottom() <= window.bottom(),
            "{pane} {pane_bounds:?} is outside {window:?}"
        );
        assert!(
            f32::from(pane_bounds.size.width).round() >= min_width,
            "{pane} {pane_bounds:?}"
        );
        assert!(
            f32::from(pane_bounds.size.height).round() >= min_height,
            "{pane} {pane_bounds:?}"
        );
    }
}

fn workspace_id(fixture: &Fixture, name: &str) -> WorkspaceId {
    fixture
        .app()
        .workspace_get()
        .open
        .into_iter()
        .find(|view| view.name == name)
        .map(|view| view.id)
        .unwrap_or_else(|| panic!("no workspace {name}"))
}

#[gpui::test]
fn ui_19_the_dock_button_moves_the_pane_and_is_kept(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let id = fixture.active();
    let (mut cx, _) = main_window(cx, &fixture);
    wait_until(&mut cx, |cx| label(cx, DOCK).is_some());

    // Right of the history by default.
    assert!(docked_right(&mut cx));
    assert_eq!(label(&mut cx, DOCK).as_deref(), Some("Dock at bottom"));
    assert!(cx.update(|window, _| window.try_find(CHANGES_SPLITTER).is_none()));

    // At the bottom: under the history, right of the tree, with a splitter
    // between them; the choice is in the workspace file.
    click(&mut cx, DOCK.into());
    wait_until(&mut cx, docked_at_bottom);
    assert_eq!(label(&mut cx, DOCK).as_deref(), Some("Dock right"));
    assert!(bounds(&mut cx, "tree").right() <= bounds(&mut cx, "history").left());
    assert!(cx.update(|window, _| window.try_find(HISTORY_SPLITTER).is_none()));
    assert_eq!(
        label(&mut cx, CHANGES_SPLITTER).as_deref(),
        Some("Resize Changes")
    );
    wait_until(&mut cx, |_| fixture.layout(&id).dock == Some(Dock::Bottom));
    assert!(fixture.workspace_file(&id).contains("dock = \"bottom\""));

    // And back.
    click(&mut cx, DOCK.into());
    wait_until(&mut cx, docked_right);
    assert_eq!(label(&mut cx, DOCK).as_deref(), Some("Dock at bottom"));
    wait_until(&mut cx, |_| fixture.layout(&id).dock == Some(Dock::Right));
}

#[gpui::test]
fn ui_19_dock_is_per_workspace(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let app = fixture.app().clone();
    fixture
        .runtime
        .block_on(async { app.workspace_create("Java".to_owned()).await.unwrap() });
    let java = workspace_id(&fixture, "Java");
    let mine = workspace_id(&fixture, "My workspace");
    fixture.set_layout(
        &java,
        PaneLayout {
            dock: Some(Dock::Bottom),
            changes: Some(200),
            ..PaneLayout::default()
        },
    );
    let (mut cx, _) = main_window(cx, &fixture);
    wait_until(&mut cx, |cx| label(cx, DOCK).is_some());

    // Java, the active tab, starts from its saved layout.
    assert!(docked_at_bottom(&mut cx));
    assert_eq!(height(&mut cx, "changes"), 200.);

    // My workspace has its own: right.
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, docked_right);
    click(&mut cx, DOCK.into());
    wait_until(&mut cx, docked_at_bottom);

    // Java is as it was left; docking it right leaves My workspace below.
    click(&mut cx, tab(&java));
    wait_until(&mut cx, |cx| {
        docked_at_bottom(cx) && height(cx, "changes") == 200.
    });
    click(&mut cx, DOCK.into());
    wait_until(&mut cx, docked_right);
    click(&mut cx, tab(&mine));
    wait_until(&mut cx, docked_at_bottom);

    wait_until(&mut cx, |_| {
        fixture.layout(&java).dock == Some(Dock::Right)
            && fixture.layout(&mine).dock == Some(Dock::Bottom)
    });
    assert_eq!(fixture.layout(&java).changes, Some(200));
}

#[gpui::test]
fn ui_19_the_changes_pane_stays_in_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::start(EVENT_CAPACITY);
    let id = fixture.active();
    let (mut cx, _) = main_window(cx, &fixture);
    wait_until(&mut cx, |cx| label(cx, DOCK).is_some());
    let panes = ["tree", "history", "changes"];

    // Docked right, the history grows until the changes pane is at its
    // minimum.
    assert_eq!(
        label(&mut cx, HISTORY_SPLITTER).as_deref(),
        Some("Resize History")
    );
    drag(&mut cx, HISTORY_SPLITTER, 2000., false);
    assert_eq!(width(&mut cx, "changes"), MIN_PANE_WIDTH);
    let wide = width(&mut cx, "history");
    wait_until(&mut cx, |_| {
        fixture.layout(&id).history == Some(wide as u32)
    });

    // A narrower window shrinks the panes to fit, each to its minimum at
    // least.
    resize(&mut cx, 760., 800.);
    assert_in_window(&mut cx, &panes, MIN_PANE_WIDTH, 0.);
    assert!(width(&mut cx, "history") < wide);

    // The saved sizes are kept: a wider window shows them again.
    resize(&mut cx, 1200., 800.);
    assert_eq!(width(&mut cx, "history"), wide);

    // A drag starts from the width shown, not the larger saved one.
    resize(&mut cx, 760., 800.);
    let shown = width(&mut cx, "history");
    drag(&mut cx, HISTORY_SPLITTER, -20., false);
    assert_eq!(width(&mut cx, "history"), shown - 20.);
    assert_in_window(&mut cx, &panes, MIN_PANE_WIDTH, 0.);
    wait_until(&mut cx, |_| {
        fixture.layout(&id).history == Some((shown - 20.) as u32)
    });

    // Docked at the bottom, the arrow keys move the focused splitter: up
    // makes the changes pane taller.
    resize(&mut cx, 1200., 1200.);
    click(&mut cx, DOCK.into());
    wait_until(&mut cx, docked_at_bottom);
    let start = height(&mut cx, "changes");
    click(&mut cx, CHANGES_SPLITTER.into());
    for _ in 0..30 {
        press(&mut cx, "up");
    }
    let tall = height(&mut cx, "changes");
    assert_eq!(tall, start + 30. * 16.);
    wait_until(&mut cx, |_| {
        fixture.layout(&id).changes == Some(tall as u32)
    });
    press(&mut cx, "down");
    assert_eq!(height(&mut cx, "changes"), tall - 16.);
    press(&mut cx, "up");

    // A shorter window keeps the history and the changes pane at their
    // minimum height; a taller one shows the saved height again.
    resize(&mut cx, 1200., 500.);
    assert_in_window(&mut cx, &panes, MIN_PANE_WIDTH, MIN_PANE_HEIGHT);
    resize(&mut cx, 1200., 1200.);
    assert_eq!(height(&mut cx, "changes"), tall);

    // A wide tree leaves room for the history and changes when the window
    // narrows.
    resize(&mut cx, 1200., 800.);
    drag(&mut cx, TREE_SPLITTER, 2000., false);
    assert_eq!(width(&mut cx, "history"), MIN_PANE_WIDTH);
    resize(&mut cx, 760., 800.);
    assert_in_window(&mut cx, &panes, MIN_PANE_WIDTH, MIN_PANE_HEIGHT);
}
