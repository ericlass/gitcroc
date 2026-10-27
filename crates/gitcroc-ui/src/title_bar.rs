//! The window's own title bar (UI-35): the row the tab bar fills, with the
//! window buttons on Windows and Linux and room for the
//! system's traffic lights on macOS. A drag on its empty space moves the
//! window and a double-click maximizes or restores it. With
//! `system_title_bar` on, the system draws the title bar and this row has
//! no buttons.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyView, App, ClickEvent, Context, Decorations, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    TestSupportExt, Window, WindowControlArea, div, px, svg,
};

use crate::icons;
use crate::strings;
use crate::theme::ActiveTokens;

/// The title bar's height; on macOS the traffic lights are centred in it.
pub const TITLE_BAR_HEIGHT: Pixels = px(34.);
/// The room the traffic lights take at the left end on macOS.
const TRAFFIC_LIGHTS_WIDTH: Pixels = px(78.);
const BUTTON_WIDTH: Pixels = px(46.);

/// What the title bar asks of the window. The app's is [`SystemControls`];
/// headless tests record the calls, since GPUI's test window cannot move,
/// minimize or maximize.
pub trait WindowControls {
    fn minimize(&self, window: &mut Window);
    /// Maximizes the window, or restores it when it is maximized.
    fn toggle_maximize(&self, window: &mut Window);
    fn close(&self, window: &mut Window, cx: &mut App);
    /// Moves the window with the pointer until the button is released.
    fn start_move(&self, window: &mut Window);
    fn is_maximized(&self, window: &Window) -> bool;
    /// Whether the app draws the window's frame. A Linux window manager may
    /// draw it anyway, with its own buttons.
    fn client_decorated(&self, window: &Window) -> bool;
}

/// The window's own controls.
pub struct SystemControls;

impl WindowControls for SystemControls {
    fn minimize(&self, window: &mut Window) {
        window.minimize_window();
    }

    fn toggle_maximize(&self, window: &mut Window) {
        window.zoom_window();
    }

    fn close(&self, window: &mut Window, _: &mut App) {
        window.remove_window();
    }

    fn start_move(&self, window: &mut Window) {
        window.start_window_move();
    }

    fn is_maximized(&self, window: &Window) -> bool {
        window.is_maximized()
    }

    fn client_decorated(&self, window: &Window) -> bool {
        !cfg!(target_os = "linux")
            || matches!(window.window_decorations(), Decorations::Client { .. })
    }
}

/// The platforms whose title bars differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// A window button of the title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl WindowButton {
    /// The button's element id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Minimize => "window-minimize",
            Self::Maximize => "window-maximize",
            Self::Restore => "window-restore",
            Self::Close => "window-close",
        }
    }

    /// What screen readers call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Minimize => strings::window::MINIMIZE,
            Self::Maximize => strings::window::MAXIMIZE,
            Self::Restore => strings::window::RESTORE,
            Self::Close => strings::window::CLOSE,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Minimize => icons::WINDOW_MINIMIZE,
            Self::Maximize => icons::WINDOW_MAXIMIZE,
            Self::Restore => icons::WINDOW_RESTORE,
            Self::Close => icons::WINDOW_CLOSE,
        }
    }

    /// Where Windows hit-tests it, which gives the maximize button its Snap
    /// Layouts.
    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

/// What the title bar holds besides its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleBarLayout {
    pub buttons: Vec<WindowButton>,
    /// Room at the left end for the traffic lights.
    pub traffic_lights: bool,
}

impl TitleBarLayout {
    /// The title bar of `platform`. With the system's title bar, or with a
    /// frame the window manager draws, the system's buttons are shown and
    /// this row has none.
    pub fn of(platform: Platform, own_frame: bool, maximized: bool) -> Self {
        match platform {
            _ if !own_frame => Self {
                buttons: Vec::new(),
                traffic_lights: false,
            },
            Platform::MacOs => Self {
                buttons: Vec::new(),
                traffic_lights: true,
            },
            Platform::Linux | Platform::Windows => Self {
                buttons: vec![
                    WindowButton::Minimize,
                    if maximized {
                        WindowButton::Restore
                    } else {
                        WindowButton::Maximize
                    },
                    WindowButton::Close,
                ],
                traffic_lights: false,
            },
        }
    }
}

/// The title bar row, a view of its own so the tab bar can fill it.
pub struct TitleBar {
    controls: Rc<dyn WindowControls>,
    platform: Platform,
    system_title_bar: bool,
    title: SharedString,
    /// The tab bar, in place of the title; screens without tabs, such as
    /// the git setup screen, show the title.
    tab_bar: Option<AnyView>,
    /// The left button went down on the empty space; the next move of the
    /// pointer starts moving the window.
    pressed: Rc<Cell<bool>>,
}

impl TitleBar {
    pub fn new(
        controls: Rc<dyn WindowControls>,
        platform: Platform,
        system_title_bar: bool,
    ) -> Self {
        Self {
            controls,
            platform,
            system_title_bar,
            title: strings::APP_NAME.into(),
            tab_bar: None,
            pressed: Rc::new(Cell::new(false)),
        }
    }

    /// Shows the tab bar in place of the title, or the title again.
    pub fn set_tab_bar(&mut self, tab_bar: Option<AnyView>, cx: &mut Context<Self>) {
        self.tab_bar = tab_bar;
        cx.notify();
    }

    /// The title it shows while there is no tab bar.
    pub fn set_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        if self.title != title {
            self.title = title;
            cx.notify();
        }
    }

    pub fn title(&self) -> &SharedString {
        &self.title
    }

    fn button(&self, button: WindowButton, cx: &App) -> impl IntoElement {
        let controls = self.controls.clone();
        let tokens = cx.tokens();
        let (hover, active, icon) = if button == WindowButton::Close {
            (tokens.danger, tokens.danger, tokens.text_primary)
        } else {
            (tokens.bg_3, tokens.border, tokens.text_primary)
        };
        // No focus handle: like the system's buttons, they are not in the
        // Tab order, and the system's shortcuts work instead.
        div()
            .id(button.id())
            .test_support()
            .role(Role::Button)
            .aria_label(button.label())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .w(BUTTON_WIDTH)
            .h_full()
            .hover(move |style| style.bg(hover))
            .active(move |style| style.bg(active))
            // On Windows the system handles the press: hit-testing the
            // button makes Snap Layouts appear over the maximize button.
            .when(self.platform == Platform::Windows, |this| {
                this.window_control_area(button.area())
            })
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match button {
                    WindowButton::Minimize => controls.minimize(window),
                    WindowButton::Maximize | WindowButton::Restore => {
                        controls.toggle_maximize(window)
                    }
                    WindowButton::Close => controls.close(window, cx),
                }
            })
            .child(svg().path(button.icon()).size(px(16.)).text_color(icon))
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let own_frame = !self.system_title_bar && self.controls.client_decorated(window);
        let layout =
            TitleBarLayout::of(self.platform, own_frame, self.controls.is_maximized(window));
        let tokens = *cx.tokens();
        let (down, up, out, moved) = (
            self.pressed.clone(),
            self.pressed.clone(),
            self.pressed.clone(),
            self.pressed.clone(),
        );
        let (drag, double_click) = (self.controls.clone(), self.controls.clone());
        div()
            .id("title-bar")
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .w_full()
            .h(TITLE_BAR_HEIGHT)
            .bg(tokens.bg_2)
            .border_b_1()
            .border_color(tokens.border)
            .when(own_frame, |this| {
                this.on_mouse_down(MouseButton::Left, move |_, _, _| down.set(true))
                    .on_mouse_up(MouseButton::Left, move |_, _, _| up.set(false))
                    .on_mouse_down_out(move |_, _, _| out.set(false))
                    .on_mouse_move(move |_, window, _| {
                        if moved.replace(false) {
                            drag.start_move(window);
                        }
                    })
                    .on_click(move |event: &ClickEvent, window, _| {
                        if event.click_count() == 2 {
                            double_click.toggle_maximize(window);
                        }
                    })
            })
            .when(layout.traffic_lights, |this| {
                this.child(
                    div()
                        .id("traffic-lights")
                        .test_support()
                        .flex_shrink_0()
                        .w(TRAFFIC_LIGHTS_WIDTH),
                )
            })
            .children(self.tab_bar.clone())
            .child(
                div()
                    .id("title-bar-space")
                    .test_support()
                    .flex()
                    .flex_1()
                    .items_center()
                    .h_full()
                    .px_3()
                    .overflow_hidden()
                    // On Windows the system moves the window and maximizes
                    // it on a double-click.
                    .when(own_frame && self.platform == Platform::Windows, |this| {
                        this.window_control_area(WindowControlArea::Drag)
                    })
                    .when(self.tab_bar.is_none(), |this| {
                        this.child(self.title.clone())
                    }),
            )
            .children(layout.buttons.iter().map(|button| self.button(*button, cx)))
    }
}
