//! The window's own title bar (UI-35).

use std::rc::Rc;

use gitcroc_ui::title_bar::{Platform, TitleBar, WindowButton};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Role, TestAppContext, VisualTestContext, point, px};

use crate::support::{Asked, FakeControls, open};

const BUTTONS: [WindowButton; 4] = [
    WindowButton::Minimize,
    WindowButton::Maximize,
    WindowButton::Restore,
    WindowButton::Close,
];

/// A window that shows only the title bar of `platform`.
fn title_bar(
    cx: &mut TestAppContext,
    platform: Platform,
    system_title_bar: bool,
    controls: &Rc<FakeControls>,
) -> VisualTestContext {
    let controls = controls.clone();
    let (cx, _) = open(cx, move |_, cx| {
        cx.new(|_| TitleBar::new(controls, platform, system_title_bar))
    });
    cx
}

/// The window buttons the title bar shows, in order.
fn buttons(cx: &mut VisualTestContext) -> Vec<WindowButton> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut shown: Vec<(f32, WindowButton)> = BUTTONS
            .into_iter()
            .filter_map(|button| {
                let found = window.try_find(button.id())?;
                Some((f32::from(found.bounds().origin.x), button))
            })
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        shown.into_iter().map(|(_, button)| button).collect()
    })
}

#[gpui::test]
fn ui_35_title_bar_buttons_per_platform(cx: &mut TestAppContext) {
    let all = [
        WindowButton::Minimize,
        WindowButton::Maximize,
        WindowButton::Close,
    ];
    let controls = Rc::new(FakeControls::default());
    for platform in [Platform::Windows, Platform::Linux] {
        let mut window = title_bar(cx, platform, false, &controls);
        assert_eq!(buttons(&mut window), all, "{platform:?}");
        // At the right end, after the title.
        let (space, close) = window.update(|window, _| {
            (
                window.find("title-bar-space").bounds(),
                window.find(WindowButton::Close.id()).bounds(),
            )
        });
        assert!(space.right() <= close.left(), "{platform:?}");
        assert_eq!(close.right(), px(1200.), "{platform:?}");
        assert!(window.update(|window, _| window.try_find("traffic-lights").is_none()));
    }

    // macOS: the system's traffic lights, with room for them at the left end.
    let mut mac = title_bar(cx, Platform::MacOs, false, &controls);
    assert_eq!(buttons(&mut mac), []);
    let (lights, space) = mac.update(|window, _| {
        (
            window.find("traffic-lights").bounds(),
            window.find("title-bar-space").bounds(),
        )
    });
    assert_eq!(lights.left(), px(0.));
    assert!(lights.size.width >= px(70.));
    assert_eq!(space.left(), lights.right());

    // The system's title bar has the buttons, on every platform.
    for platform in [Platform::Windows, Platform::Linux, Platform::MacOs] {
        let mut window = title_bar(cx, platform, true, &controls);
        assert_eq!(buttons(&mut window), [], "{platform:?}");
        assert!(window.update(|window, _| window.try_find("traffic-lights").is_none()));
    }

    // A Linux window manager that draws the frame draws its buttons too.
    let server = Rc::new(FakeControls::default());
    server.server_decorated.set(true);
    let mut window = title_bar(cx, Platform::Linux, false, &server);
    assert_eq!(buttons(&mut window), []);

    // Maximized, the middle button restores.
    let maximized = Rc::new(FakeControls::default());
    maximized.maximized.set(true);
    let mut window = title_bar(cx, Platform::Windows, false, &maximized);
    assert_eq!(
        buttons(&mut window),
        [
            WindowButton::Minimize,
            WindowButton::Restore,
            WindowButton::Close
        ]
    );
    maximized.maximized.set(false);
    window.update(|window, _| window.refresh());
    assert_eq!(buttons(&mut window), all);
}

#[gpui::test]
fn ui_35_title_bar_drag_and_double_click(cx: &mut TestAppContext) {
    let controls = Rc::new(FakeControls::default());
    let mut window = title_bar(cx, Platform::Linux, false, &controls);
    let space = window.update(|window, _| window.find("title-bar-space").bounds());

    // A press and a move on the empty space move the window.
    let from = space.center();
    window.update(|window, cx| window.drag(from, from + point(px(40.), px(10.)), cx));
    assert_eq!(*controls.asked.borrow(), [Asked::Move]);
    controls.asked.borrow_mut().clear();

    // A press without a move does not.
    window.update(|window, cx| window.click("title-bar-space", cx));
    assert_eq!(*controls.asked.borrow(), []);

    // A double-click maximizes or restores.
    window.update(|window, cx| window.double_click("title-bar-space", cx));
    assert_eq!(*controls.asked.borrow(), [Asked::ToggleMaximize]);
    controls.asked.borrow_mut().clear();

    // A press on a button does neither, and the button does its own work.
    for (button, asked) in [
        (WindowButton::Minimize, Asked::Minimize),
        (WindowButton::Maximize, Asked::ToggleMaximize),
        (WindowButton::Close, Asked::Close),
    ] {
        let center = window.update(|window, _| window.find(button.id()).bounds().center());
        window.update(|window, cx| window.drag(center, center + point(px(-30.), px(5.)), cx));
        assert_eq!(*controls.asked.borrow(), [], "{button:?} dragged");
        window.update(|window, cx| window.double_click(button.id(), cx));
        assert_eq!(*controls.asked.borrow(), [asked, asked], "{button:?}");
        controls.asked.borrow_mut().clear();
    }
}

#[gpui::test]
fn ui_35_window_buttons_are_labelled_and_not_in_the_tab_order(cx: &mut TestAppContext) {
    let controls = Rc::new(FakeControls::default());
    let mut window = title_bar(cx, Platform::Windows, false, &controls);
    for (button, label) in [
        (WindowButton::Minimize, "Minimize"),
        (WindowButton::Maximize, "Maximize"),
        (WindowButton::Close, "Close"),
    ] {
        let found = window.update(|window, _| window.find(button.id()));
        assert_eq!(found.role(), Some(Role::Button), "{button:?}");
        assert_eq!(found.label(), Some(label), "{button:?}");
        // No focus handle, so the button can never take the focus.
        assert_eq!(found.focused(), None, "{button:?}");
    }
    for _ in 0..4 {
        window.update(|window, cx| window.press("tab", cx));
        assert!(window.update(|window, cx| window.focused(cx).is_none()));
    }

    let maximized = Rc::new(FakeControls::default());
    maximized.maximized.set(true);
    let mut window = title_bar(cx, Platform::Linux, false, &maximized);
    let restore = window.update(|window, _| window.find(WindowButton::Restore.id()));
    assert_eq!(restore.role(), Some(Role::Button));
    assert_eq!(restore.label(), Some("Restore"));
}
