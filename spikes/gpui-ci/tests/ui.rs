//! Headless GPUI tests: GPUI's test platform, no window server and no GPU.

extern crate gpui_kit as gpui;

use gpui_ci_spike::Spike;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Entity, IntoElement, Modifiers,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, WindowBounds,
    WindowOptions, div, point, px, size,
};

fn open<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> (VisualTestContext, Entity<V>) {
    cx.update(gpui_kit::init);
    let (window, view): (AnyWindowHandle, Entity<V>) = cx
        .update(|cx| {
            let bounds = Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(800.), px(600.)),
            };
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                build,
            )
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(window, cx);
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    (cx, view)
}

#[gpui::test]
fn clicking_the_button_counts(cx: &mut TestAppContext) {
    let (mut cx, spike) = open(cx, |_, cx| cx.new(|_| Spike::new(None)));
    let center = cx.update(|window, _| window.find("count").bounds().center());

    cx.simulate_click(center, Modifiers::default());
    cx.simulate_click(center, Modifiers::default());
    cx.run_until_parked();

    cx.update(|_, cx| assert_eq!(spike.read(cx).clicks, 2));
}

struct Note {
    text: Entity<InputState>,
}

impl Render for Note {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_4()
            .child(Input::new(&self.text).id("note"))
    }
}

#[gpui::test]
fn typing_into_an_input(cx: &mut TestAppContext) {
    let (mut cx, note) = open(cx, |window, cx| {
        let text = cx.new(|cx| InputState::new(window, cx));
        cx.new(|_| Note { text })
    });

    cx.update(|window, cx| window.click("note", cx));
    cx.simulate_input("héllo, GPUI");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();

    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("note").value(), Some("héllo, GPU"));
        assert_eq!(note.read(cx).text.read(cx).value(), "héllo, GPU");
    });
}
