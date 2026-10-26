//! Throwaway spike: gitcroc's history pane and a commit diff in GPUI +
//! gpui-kit, reading through gitcroc-git, with a scripted scroll that
//! records frame times like `bench-ui` does (100 px a frame, from the top
//! to the end).
//!
//!   cargo run --release -- [--repo <path>] [--commit <hash>] [--bench] [--frames <n>]
//!
//! Keys: `b` runs the benchmark, `m` measures your own scrolling.

extern crate gpui_kit as gpui;

mod data;
mod stats;

use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gitcroc_git::{GitRepo, GraphRow, LineKind, RefKind};
use gpui_kit::component::{ActiveTheme, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use data::{DiffLine, Row};
use stats::Scenario;

/// As `ROW_HEIGHT` in ui/src/history/HistoryPane.tsx.
const ROW_H: f32 = 24.0;
const LINE_H: f32 = 20.0;
const LANE_W: f32 = 12.0;
const MAX_GRAPH_W: f32 = 240.0;
/// As `PREFETCH_ROWS` in the Tauri UI.
const PREFETCH_ROWS: usize = 100;
/// Rows shown as placeholders below the loaded ones while a page loads.
const PLACEHOLDERS: usize = 50;
/// As `SCROLL_PX_PER_FRAME` in ui/bench/bench-ui.ts.
const SCROLL_PX: f32 = 100.0;
/// Files read per background batch of a diff.
const DIFF_BATCH: usize = 50;
/// The diff pane stops reading past this many lines.
const MAX_DIFF_LINES: usize = 400_000;

actions!(spike, [RunBench, ToggleMeasure]);

struct Args {
    repo: PathBuf,
    commit: Option<String>,
    bench: bool,
    frames: usize,
}

fn args() -> Result<Args, String> {
    let mut parsed = Args {
        repo: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/bench-data/target-project/repos/repo-01"),
        commit: None,
        bench: false,
        frames: 10_000,
    };
    let mut rest = std::env::args().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--repo" => parsed.repo = rest.next().ok_or("--repo needs a path")?.into(),
            "--commit" => parsed.commit = Some(rest.next().ok_or("--commit needs a hash")?),
            "--bench" => parsed.bench = true,
            "--frames" => {
                parsed.frames = rest
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("--frames needs a number")?
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(parsed)
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// Let a few frames pass before measuring `next`.
    Settle { frames: u32, next: Pane },
    Scroll(Pane),
    /// Waiting for the diff of the bench commit to start arriving.
    OpenDiff,
    Done,
}

#[derive(Clone, Copy, PartialEq)]
enum Pane {
    History,
    Diff,
}

struct Bench {
    phase: Phase,
    offset: f32,
    last: Option<Instant>,
    frames: Vec<f32>,
    blank_since: Option<Instant>,
    longest_blank: Duration,
    results: Vec<Scenario>,
}

impl Bench {
    fn new() -> Self {
        Self {
            phase: Phase::Settle {
                frames: 10,
                next: Pane::History,
            },
            offset: 0.0,
            last: None,
            frames: Vec::new(),
            blank_since: None,
            longest_blank: Duration::ZERO,
            results: Vec::new(),
        }
    }

    fn finish(&mut self, name: &'static str, reached_end: bool, items: usize, cpu: &RefCell<Vec<f32>>) {
        if let Some(since) = self.blank_since.take() {
            self.longest_blank = self.longest_blank.max(since.elapsed());
        }
        self.results.push(Scenario {
            name,
            frames: std::mem::take(&mut self.frames),
            cpu: std::mem::take(&mut *cpu.borrow_mut()),
            longest_blank: std::mem::take(&mut self.longest_blank),
            distance_px: self.offset,
            reached_end,
            items,
        });
        self.offset = 0.0;
        self.last = None;
    }
}

struct Spike {
    repo: GitRepo,
    repo_name: SharedString,
    focus: FocusHandle,

    rows: Vec<Row>,
    cursor: Option<gitcroc_git::Cursor>,
    started: bool,
    done: bool,
    loading: bool,
    graph_w: f32,
    history_scroll: UniformListScrollHandle,
    selected: Option<usize>,

    diff_title: SharedString,
    diff: Vec<DiffLine>,
    diff_loading: bool,
    diff_task: Option<Task<()>>,
    diff_scroll: UniformListScrollHandle,

    bench: Option<Bench>,
    bench_commit: Option<String>,
    max_frames: usize,
    exit_after_bench: bool,
    status: SharedString,

    measure: Option<Vec<f32>>,
    last_render: Option<Instant>,
    /// CPU time of each frame: from the start of `render` to the end of
    /// painting the window's content.
    cpu: Rc<RefCell<Vec<f32>>>,
}

impl Spike {
    fn new(args: &Args, repo: GitRepo, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut spike = Self {
            repo,
            repo_name: args.repo.display().to_string().into(),
            focus,
            rows: Vec::new(),
            cursor: None,
            started: false,
            done: false,
            loading: false,
            graph_w: LANE_W * 2.0,
            history_scroll: UniformListScrollHandle::new(),
            selected: None,
            diff_title: "Select a commit".into(),
            diff: Vec::new(),
            diff_loading: false,
            diff_task: None,
            diff_scroll: UniformListScrollHandle::new(),
            bench: None,
            bench_commit: args.commit.clone(),
            max_frames: args.frames,
            exit_after_bench: args.bench,
            status: "b: benchmark   m: measure your own scrolling".into(),
            measure: None,
            last_render: None,
            cpu: Rc::default(),
        };
        spike.load_more(cx);
        if args.bench {
            spike.bench = Some(Bench::new());
        } else if let Some(hash) = &args.commit {
            spike.open_diff(hash.clone(), cx);
        }
        spike
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.done {
            return;
        }
        self.loading = true;
        let repo = self.repo.clone();
        let cursor = self.cursor.take();
        let task = cx.background_spawn(async move { data::history_page(&repo, cursor) });
        cx.spawn(async move |this, cx| {
            let page = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                this.started = true;
                match page {
                    Ok(page) => {
                        let widest = page.rows.iter().map(|row| row.lanes).max().unwrap_or(0);
                        this.graph_w = this
                            .graph_w
                            .max(widest as f32 * LANE_W)
                            .min(MAX_GRAPH_W);
                        this.rows.extend(page.rows);
                        this.done = page.next.is_none();
                        this.cursor = page.next;
                    }
                    Err(error) => {
                        this.done = true;
                        this.status = format!("history: {error}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(ix) else { return };
        self.selected = Some(ix);
        self.open_diff(row.hash.clone(), cx);
    }

    fn open_diff(&mut self, hash: String, cx: &mut Context<Self>) {
        let title = self
            .rows
            .iter()
            .find(|row| row.hash == hash)
            .map_or_else(|| hash.clone(), |row| format!("{}  {}", row.short_hash, row.subject));
        self.diff_title = title.into();
        self.diff.clear();
        self.diff_loading = true;
        self.diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
        let repo = self.repo.clone();
        // Dropping the previous task cancels it.
        self.diff_task = Some(cx.spawn(async move |this, cx| {
            let files = {
                let repo = repo.clone();
                let hash = hash.clone();
                cx.background_spawn(async move { data::changed_files(&repo, &hash) })
                    .await
            };
            let files = match files {
                Ok(files) => files,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.diff.push(DiffLine::Note(error.into()));
                        this.diff_loading = false;
                        cx.notify();
                    });
                    return;
                }
            };
            let count = files.len();
            let mut batches = files.chunks(DIFF_BATCH).map(<[_]>::to_vec).collect::<Vec<_>>().into_iter();
            let mut total = 0;
            while let Some(batch) = batches.next() {
                let repo = repo.clone();
                let hash = hash.clone();
                let lines = cx
                    .background_spawn(async move { data::diff_lines(&repo, &hash, &batch) })
                    .await;
                total += lines.len();
                let last = batches.len() == 0 || total >= MAX_DIFF_LINES;
                let alive = this.update(cx, |this, cx| {
                    this.diff.extend(lines);
                    if last {
                        this.diff_loading = false;
                        if total >= MAX_DIFF_LINES {
                            this.diff.push(DiffLine::Note(
                                format!("Stopped reading at {MAX_DIFF_LINES} lines ({count} files changed)").into(),
                            ));
                        }
                    }
                    cx.notify();
                });
                if alive.is_err() || last {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn run_bench(&mut self, _: &RunBench, window: &mut Window, cx: &mut Context<Self>) {
        if self.bench.is_none() {
            self.bench = Some(Bench::new());
            window.request_animation_frame();
            cx.notify();
        }
    }

    fn toggle_measure(&mut self, _: &ToggleMeasure, _: &mut Window, cx: &mut Context<Self>) {
        self.measure = match self.measure {
            Some(_) => None,
            None => Some(Vec::new()),
        };
        self.cpu.borrow_mut().clear();
        self.status = if self.measure.is_some() {
            "Measuring: scroll either pane".into()
        } else {
            "b: benchmark   m: measure your own scrolling".into()
        };
        cx.notify();
    }

    /// The display's refresh interval; X11 does not report it to GPUI, so
    /// then the median of `frames`, the vsync-paced interval.
    fn refresh_ms(window: &Window, cx: &App, frames: &[f32]) -> f32 {
        window.refresh_interval(cx).map_or_else(
            || stats::summarize(frames, 0.0).map_or(1000.0 / 60.0, |s| s.p50),
            |interval| interval.as_secs_f32() * 1000.0,
        )
    }

    /// Records the frame and moves the scripted scroll on by one step.
    fn step_bench(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut bench) = self.bench.take() else { return };
        let now = Instant::now();
        match bench.phase {
            Phase::Settle { frames, next } => {
                if frames == 0 && (next == Pane::Diff || self.started) {
                    bench.phase = Phase::Scroll(next);
                    bench.last = Some(now);
                    self.cpu.borrow_mut().clear();
                } else {
                    bench.phase = Phase::Settle {
                        frames: frames.saturating_sub(1),
                        next,
                    };
                }
                let handle = match next {
                    Pane::History => &self.history_scroll,
                    Pane::Diff => &self.diff_scroll,
                };
                handle.0.borrow().base_handle.set_offset(point(px(0.), px(0.)));
            }
            Phase::Scroll(pane) => {
                if let Some(last) = bench.last.replace(now) {
                    bench.frames.push((now - last).as_secs_f32() * 1000.0);
                }
                let (handle, loaded, loading, row_h) = match pane {
                    Pane::History => (&self.history_scroll, self.rows.len(), !self.done, ROW_H),
                    Pane::Diff => (&self.diff_scroll, self.diff.len(), self.diff_loading, LINE_H),
                };
                let state = handle.0.borrow();
                let max = f32::from(state.base_handle.max_offset().y);
                let height = f32::from(state.base_handle.bounds().size.height);
                bench.offset = (bench.offset + SCROLL_PX).min(max.max(0.0));
                state.base_handle.set_offset(point(px(0.), px(-bench.offset)));
                drop(state);
                let last_visible = ((bench.offset + height) / row_h).ceil() as usize;
                let blank = last_visible > loaded && loading;
                if blank {
                    bench.blank_since.get_or_insert(now);
                } else if let Some(since) = bench.blank_since.take() {
                    bench.longest_blank = bench.longest_blank.max(now - since);
                }
                let at_end = !loading && bench.offset >= max - 0.5;
                if at_end || bench.frames.len() >= self.max_frames {
                    let name = match pane {
                        Pane::History => "history-scroll",
                        Pane::Diff => "diff-scroll",
                    };
                    bench.finish(name, at_end, loaded, &self.cpu);
                    bench.phase = match pane {
                        Pane::History => Phase::OpenDiff,
                        Pane::Diff => Phase::Done,
                    };
                }
            }
            Phase::OpenDiff => {
                if !self.diff_loading || self.diff.is_empty() {
                    let commit = self
                        .bench_commit
                        .clone()
                        .or_else(|| self.rows.last().map(|row| row.hash.clone()));
                    match commit {
                        Some(hash) if self.diff_task.is_none() || !self.diff_loading => {
                            if self.diff.is_empty() || !self.diff_title.contains(&hash[..7.min(hash.len())]) {
                                self.selected = self.rows.iter().position(|row| row.hash == hash);
                                self.open_diff(hash, cx);
                            }
                        }
                        Some(_) => {}
                        None => bench.phase = Phase::Done,
                    }
                }
                if !self.diff.is_empty() {
                    bench.phase = Phase::Settle {
                        frames: 10,
                        next: Pane::Diff,
                    };
                }
            }
            Phase::Done => {
                let all: Vec<f32> = bench.results.iter().flat_map(|s| s.frames.iter().copied()).collect();
                let refresh = Self::refresh_ms(window, cx, &all);
                let size = window.viewport_size();
                let context = [
                    ("repo", format!("\"{}\"", self.repo_name)),
                    ("refresh_ms", format!("{refresh:.3}")),
                    (
                        "window_px",
                        format!(
                            "[{:.0}, {:.0}]",
                            f32::from(size.width) * window.scale_factor(),
                            f32::from(size.height) * window.scale_factor()
                        ),
                    ),
                    ("scale_factor", format!("{}", window.scale_factor())),
                    ("px_per_frame", format!("{SCROLL_PX}")),
                ];
                println!("{}", stats::json(&bench.results, &context, refresh));
                self.status = bench
                    .results
                    .iter()
                    .map(|scenario| {
                        stats::summarize(&scenario.frames, refresh).map_or_else(
                            || format!("{}: no frames", scenario.name),
                            |s| {
                                let cpu = stats::summarize(&scenario.cpu, refresh).map_or(0.0, |c| c.p95);
                                format!(
                                    "{}: {:.0} fps, p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms, {:.1}% janky, CPU p95 {:.1} ms",
                                    scenario.name, s.fps, s.p50, s.p95, s.max, s.janky, cpu
                                )
                            },
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("   |   ")
                    .into();
                if self.exit_after_bench {
                    cx.quit();
                }
                return;
            }
        }
        self.status = match bench.phase {
            Phase::Scroll(Pane::History) => format!("Benchmark: history, frame {}", bench.frames.len()),
            Phase::Scroll(Pane::Diff) => format!("Benchmark: diff, frame {}", bench.frames.len()),
            _ => "Benchmark: preparing".into(),
        }
        .into();
        self.bench = Some(bench);
        window.request_animation_frame();
    }

    fn record_manual(&mut self, window: &Window, cx: &App) {
        let now = Instant::now();
        let last = self.last_render.replace(now);
        let Some(samples) = self.measure.as_mut() else { return };
        if let Some(last) = last {
            let ms = (now - last).as_secs_f32() * 1000.0;
            // Gaps between scroll gestures are not frames.
            if ms < 100.0 {
                samples.push(ms);
            }
        }
        let refresh = Self::refresh_ms(window, cx, samples);
        if let Some(s) = stats::summarize(samples, refresh) {
            let cpu = stats::summarize(&self.cpu.borrow(), refresh).map_or(0.0, |c| c.p95);
            self.status = format!(
                "Measuring {} frames: {:.0} fps, p50 {:.1} ms, p95 {:.1} ms, p99 {:.1} ms, max {:.1} ms, {:.1}% janky, CPU p95 {:.1} ms   (m stops)",
                samples.len(),
                s.fps,
                s.p50,
                s.p95,
                s.p99,
                s.max,
                s.janky,
                cpu
            )
            .into();
        }
    }

    fn history_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !self.done && range.end + PREFETCH_ROWS >= self.rows.len() {
            self.load_more(cx);
        }
        let theme = cx.theme();
        let colors = graph_colors(theme.is_dark());
        range
            .map(|ix| {
                let Some(row) = self.rows.get(ix) else {
                    return h_flex()
                        .w_full()
                        .h(px(ROW_H))
                        .px_3()
                        .child(div().h(px(10.)).w(relative(0.4)).rounded(px(3.)).bg(theme.muted))
                        .into_any_element();
                };
                let graph = row.graph.clone();
                let selected = self.selected == Some(ix);
                h_flex()
                    .id(ix)
                    .w_full()
                    .h(px(ROW_H))
                    .px_2()
                    .gap_3()
                    .text_size(px(13.))
                    .when(selected, |row| row.bg(theme.list_active))
                    .when(!selected, |row| row.hover(|style| style.bg(theme.list_hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(ix, cx)))
                    .child(
                        div().w(px(self.graph_w)).h(px(ROW_H)).flex_none().child(
                            canvas(|_, _, _| (), move |bounds, (), window, _| {
                                if let Some(graph) = &graph {
                                    paint_graph(graph, bounds, &colors, window);
                                }
                            })
                            .size_full(),
                        ),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .overflow_hidden()
                            .children(row.refs.iter().map(|(kind, name)| ref_label(*kind, name, theme)))
                            .child(div().truncate().text_color(theme.foreground).child(row.subject.clone())),
                    )
                    .child(
                        div()
                            .w(px(170.))
                            .flex_none()
                            .truncate()
                            .text_color(theme.muted_foreground)
                            .child(row.author.clone()),
                    )
                    .child(
                        div()
                            .w(px(120.))
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(theme.muted_foreground)
                            .child(row.date.clone()),
                    )
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .whitespace_nowrap()
                            .font_family(theme.mono_font_family.clone())
                            .text_size(px(12.))
                            .text_color(theme.muted_foreground)
                            .child(row.short_hash.clone()),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn diff_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme();
        let mono = theme.mono_font_family.clone();
        let added = theme.success.opacity(0.16);
        let removed = theme.danger.opacity(0.16);
        range
            .map(|ix| match self.diff.get(ix) {
                None => h_flex()
                    .w_full()
                    .h(px(LINE_H))
                    .px_3()
                    .child(div().h(px(8.)).w(relative(0.3)).rounded(px(3.)).bg(theme.muted))
                    .into_any_element(),
                Some(DiffLine::File { path, stats }) => h_flex()
                    .w_full()
                    .h(px(LINE_H))
                    .px_3()
                    .gap_3()
                    .bg(theme.secondary)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_size(px(12.5))
                    .child(div().font_weight(FontWeight::SEMIBOLD).truncate().child(path.clone()))
                    .child(div().text_color(theme.muted_foreground).child(stats.clone()))
                    .into_any_element(),
                Some(DiffLine::Hunk(text)) => div()
                    .w_full()
                    .h(px(LINE_H))
                    .whitespace_nowrap()
                    .pl(px(116.))
                    .font_family(mono.clone())
                    .text_size(px(12.))
                    .text_color(theme.info)
                    .bg(theme.info.opacity(0.07))
                    .child(text.clone())
                    .into_any_element(),
                Some(DiffLine::Note(text)) => div()
                    .w_full()
                    .h(px(LINE_H))
                    .whitespace_nowrap()
                    .pl(px(116.))
                    .text_size(px(12.))
                    .italic()
                    .text_color(theme.muted_foreground)
                    .child(text.clone())
                    .into_any_element(),
                Some(DiffLine::Text { kind, old, new, text }) => {
                    let (bg, sign) = match kind {
                        LineKind::Added => (Some(added), "+"),
                        LineKind::Removed => (Some(removed), "−"),
                        LineKind::Context => (None, " "),
                    };
                    let number = |n: &SharedString| {
                        div()
                            .w(px(44.))
                            .flex_none()
                            .pr_2()
                            .text_right()
                            .text_color(theme.muted_foreground.opacity(0.7))
                            .child(n.clone())
                    };
                    h_flex()
                        .w_full()
                        .h(px(LINE_H))
                        .overflow_hidden()
                        .font_family(mono.clone())
                        .text_size(px(12.5))
                        .when_some(bg, |line, bg| line.bg(bg))
                        .child(number(old))
                        .child(number(new))
                        .child(div().w(px(20.)).flex_none().text_color(theme.muted_foreground).child(sign))
                        .child(div().whitespace_nowrap().text_color(theme.foreground).child(text.clone()))
                        .into_any_element()
                }
            })
            .collect()
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_start = Instant::now();
        self.record_manual(window, cx);
        self.step_bench(window, cx);

        let theme = cx.theme();
        let history_count = self.rows.len() + if self.done { 0 } else { PLACEHOLDERS };
        let diff_count = self.diff.len() + if self.diff_loading { PLACEHOLDERS } else { 0 };
        let header = |label: &'static str, width: Option<f32>| {
            div()
                .when_some(width, |cell, width| cell.w(px(width)).flex_none())
                .when(width.is_none(), |cell| cell.flex_1())
                .child(label)
        };

        v_flex()
            .key_context("Spike")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::run_bench))
            .on_action(cx.listener(Self::toggle_measure))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .child(
                h_flex()
                    .h(px(36.))
                    .px_3()
                    .gap_4()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.secondary)
                    .text_size(px(12.5))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("gitcroc · GPUI spike"))
                    .child(div().text_color(theme.muted_foreground).truncate().child(self.repo_name.clone()))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(format!("{} commits{}", self.rows.len(), if self.done { "" } else { "+" })),
                    )
                    .child(div().flex_1())
                    .child(div().text_color(theme.primary).truncate().child(self.status.clone())),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .w(relative(0.55))
                            .h_full()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                h_flex()
                                    .h(px(28.))
                                    .px_2()
                                    .gap_3()
                                    .flex_none()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .text_size(px(12.))
                                    .text_color(theme.muted_foreground)
                                    .child(header("Graph", Some(self.graph_w)))
                                    .child(header("Subject", None))
                                    .child(header("Author", Some(170.)))
                                    .child(header("Date", Some(120.)))
                                    .child(header("Hash", Some(84.))),
                            )
                            .child(
                                uniform_list("history", history_count, cx.processor(Self::history_rows_proc))
                                    .track_scroll(&self.history_scroll)
                                    .flex_1(),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(
                                h_flex()
                                    .h(px(28.))
                                    .px_3()
                                    .flex_none()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .text_size(px(12.5))
                                    .child(div().truncate().child(self.diff_title.clone())),
                            )
                            .child(
                                uniform_list("diff", diff_count, cx.processor(Self::diff_rows_proc))
                                    .track_scroll(&self.diff_scroll)
                                    .flex_1(),
                            ),
                    ),
            )
            // Painted last: the end of this frame's CPU work.
            .child({
                let cpu = self.cpu.clone();
                let measuring = self.measure.is_some() || self.bench.is_some();
                canvas(|_, _, _| (), move |_, (), _, _| {
                    if measuring {
                        cpu.borrow_mut().push(render_start.elapsed().as_secs_f32() * 1000.0);
                    }
                })
                .absolute()
                .size_0()
            })
    }
}

impl Spike {
    fn history_rows_proc(&mut self, range: Range<usize>, _: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.history_rows(range, cx)
    }

    fn diff_rows_proc(&mut self, range: Range<usize>, _: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.diff_rows(range, cx)
    }
}

fn ref_label(kind: RefKind, name: &SharedString, theme: &Theme) -> AnyElement {
    let color = match kind {
        RefKind::Head => theme.danger,
        RefKind::CurrentBranch => theme.success,
        RefKind::LocalBranch => theme.primary,
        RefKind::RemoteBranch => theme.info,
        RefKind::Tag => theme.warning,
    };
    let text: SharedString = if kind == RefKind::CurrentBranch {
        format!("✓ {name}").into()
    } else {
        name.clone()
    };
    div()
        .flex_none()
        .px(px(5.))
        .rounded(px(4.))
        .text_size(px(11.))
        .bg(color.opacity(0.14))
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// The graph colors of ui/src/theme/tokens.css.
fn graph_colors(dark: bool) -> [Hsla; 8] {
    let hex: [u32; 8] = if dark {
        [0x4493f8, 0x3fb950, 0xab7df8, 0xf0883e, 0xf778ba, 0x39c5cf, 0xf85149, 0xd29922]
    } else {
        [0x0969da, 0x1a7f37, 0x8250df, 0xbc4c00, 0xbf3989, 0x1b7c83, 0xcf222e, 0x9a6700]
    };
    hex.map(|value| rgb(value).into())
}

fn fill_quad(bounds: Bounds<Pixels>, color: Hsla) -> PaintQuad {
    fill(bounds, color)
}

/// One row's lines and dot, from the lanes the core computed.
fn paint_graph(graph: &GraphRow, bounds: Bounds<Pixels>, colors: &[Hsla; 8], window: &mut Window) {
    let x = |lane: u32| bounds.origin.x + px(LANE_W / 2.0 + lane as f32 * LANE_W);
    let top = bounds.origin.y;
    let middle = top + px(ROW_H / 2.0);
    let bottom = top + px(ROW_H);
    // Lanes past the column's right edge are not drawn at all.
    let lanes = (f32::from(bounds.size.width) / LANE_W).ceil() as u32;
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for (color, fill) in colors.iter().enumerate() {
            let mut path = PathBuilder::stroke(px(1.5));
            let mut any = false;
            for (edges, from_y, to_y) in [(&graph.up, top, middle), (&graph.down, middle, bottom)] {
                for edge in edges
                    .iter()
                    .filter(|edge| edge.color as usize % 8 == color && edge.from.min(edge.to) < lanes)
                {
                    if edge.from == edge.to {
                        // Most edges pass straight down: a quad, no tessellation.
                        let line = Bounds::new(point(x(edge.from) - px(0.75), from_y), size(px(1.5), to_y - from_y));
                        window.paint_quad(fill_quad(line, *fill));
                    } else {
                        path.move_to(point(x(edge.from), from_y));
                        path.line_to(point(x(edge.to), to_y));
                        any = true;
                    }
                }
            }
            if any && let Ok(path) = path.build() {
                window.paint_path(path, *fill);
            }
        }
        let dot = Bounds::new(point(x(graph.lane) - px(4.), middle - px(4.)), size(px(8.), px(8.)));
        window.paint_quad(fill(dot, colors[graph.color as usize % 8]).corner_radii(px(4.)));
    });
}

fn main() {
    let args = match args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let repo = match GitRepo::open(&args.repo) {
        Ok(repo) => repo,
        Err(error) => {
            eprintln!("cannot open {}: {error}", args.repo.display());
            std::process::exit(2);
        }
    };
    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        cx.bind_keys([
            KeyBinding::new("b", RunBench, Some("Spike")),
            KeyBinding::new("m", ToggleMeasure, Some("Spike")),
        ]);
        // The whole primary display: X11 window managers may ignore a
        // maximized request.
        let bounds = cx
            .primary_display()
            .map_or_else(|| Bounds::centered(None, size(px(1600.), px(1000.)), cx), |display| display.bounds());
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("gitcroc GPUI spike".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = gpui_kit::open_window(options, cx, |window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
            window
                .observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx))
                .detach();
            cx.new(|cx| Spike::new(&args, repo, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("cannot open the window: {error}");
            cx.quit();
        }
    });
}
