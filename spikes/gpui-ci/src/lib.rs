//! Throwaway spike (M6-03): one window with a list of 10,000 rows and a
//! button, to prove GPUI runs on GitHub's hosted runners.

extern crate gpui_kit as gpui;

pub mod update;

use std::ops::Range;
use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::*;

pub const ROWS: usize = 10_000;
const ROW_H: f32 = 24.0;
/// As the history spike's scripted scroll.
const SCROLL_PX: f32 = 100.0;

/// Called with the frame times once a `--frames` run has its frames.
type Done = Box<dyn FnOnce(Vec<f32>, &mut Window, &mut App)>;

/// The frame times of a `--frames` run, in ms between consecutive frames.
pub struct FrameRun {
    target: usize,
    last: Option<Instant>,
    pub frames: Vec<f32>,
    offset: f32,
    done: Option<Done>,
}

impl FrameRun {
    pub fn new(
        target: usize,
        done: impl FnOnce(Vec<f32>, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            target,
            last: None,
            frames: Vec::with_capacity(target),
            offset: 0.0,
            done: Some(Box::new(done)),
        }
    }
}

pub struct Spike {
    pub clicks: usize,
    scroll: UniformListScrollHandle,
    run: Option<FrameRun>,
}

impl Spike {
    pub fn new(run: Option<FrameRun>) -> Self {
        Self {
            clicks: 0,
            scroll: UniformListScrollHandle::new(),
            run,
        }
    }

    /// Records the frame and scrolls on by one step, wrapping at the end.
    fn step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.run.as_mut() else { return };
        let now = Instant::now();
        if let Some(last) = run.last.replace(now) {
            run.frames.push((now - last).as_secs_f32() * 1000.0);
        }
        let state = self.scroll.0.borrow();
        let max = f32::from(state.base_handle.max_offset().y);
        run.offset += SCROLL_PX;
        if run.offset > max {
            run.offset = 0.0;
        }
        state.base_handle.set_offset(point(px(0.), px(-run.offset)));
        drop(state);
        if run.frames.len() >= run.target {
            let frames = std::mem::take(&mut run.frames);
            if let Some(done) = run.done.take() {
                cx.defer_in(window, move |_, window, cx| done(frames, window, cx));
            }
            self.run = None;
        } else {
            window.request_animation_frame();
        }
    }

    fn rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        range
            .map(|ix| {
                h_flex()
                    .id(ix)
                    .h(px(ROW_H))
                    .px_3()
                    .gap_3()
                    .child(div().w(px(60.)).text_color(muted).child(format!("{ix}")))
                    .child(format!("Row {ix} of the GPUI CI spike"))
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.step(window, cx);
        let theme = cx.theme();
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(px(13.))
            .child(
                h_flex()
                    .p_2()
                    .gap_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("count")
                            .primary()
                            .label("Count")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clicks += 1;
                                cx.notify();
                            })),
                    )
                    .child(format!("Clicked {} times", self.clicks)),
            )
            .child(
                uniform_list("rows", ROWS, cx.processor(Self::rows))
                    .track_scroll(&self.scroll)
                    .flex_1(),
            )
    }
}
