//! The UI-side benchmark runner (PERF-8): plays a script of user actions in
//! the real window and records how long each took to show, and the frame
//! times while scrolling. `gitcroc-next --bench <script>` runs it in the
//! `perf-trace` build; `gitcroc-cli bench-ui` writes the script and reads
//! the trace (M6-14b).
//!
//! The runner works one step per frame: GPUI calls [`Runner::tick`] at the
//! start of each frame, before it draws, so a tick sees what the frame
//! before it presented. Actions are real clicks and wheel events at the
//! elements the last frame drew, so they go through the same handlers as a
//! user's. An action's time runs from its click to the tick after the frame
//! that first drew its result, that is, to that frame's present.

use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime};

use gitcroc_core::{RepoId, WorkspaceId};
use gpui_kit::base::test_support::{self as observed, ElementSnapshot};
use gpui_kit::{
    App, ElementId, Entity, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PlatformInput, Point, Role, ScrollDelta, ScrollEvent, Window, point, px,
};
use serde::{Deserialize, Serialize};

use crate::strings;
use crate::window::GitcrocWindow;

/// How far one frame scrolls, as rc.1's `scroll-frame` and the M6 history
/// spike did.
pub const SCROLL_STEP: f32 = 100.;

/// How long a step may take before the run fails.
const STEP_TIMEOUT: Duration = Duration::from_secs(60);

/// Frames a scroll may take before the run fails: 5,000 commits or 15,000
/// diff lines take under 3,000.
const MAX_SCROLL_FRAMES: usize = 20_000;

/// Frames without movement after which a diff is at its end.
const STILL_FRAMES: usize = 3;

/// What `gitcroc-cli bench-ui` asks the app to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// When the driver started the process, in milliseconds since the Unix
    /// epoch, for `cold-start`.
    pub started_ms: Option<u64>,
    pub steps: Vec<Step>,
}

/// One step. A step with a scenario name is timed under that name; the
/// others only bring the window where the next timed step starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum Step {
    /// From the process start to the first frame that shows the tree with
    /// every repo's cached summary.
    ColdStart { scenario: String },
    /// Waits until every repo of the active tab is read in this run and
    /// nothing is loading, so the next step measures the UI, not the start
    /// refresh.
    Idle,
    /// Clicks a repo in the tree, until its first history page shows.
    SelectRepo {
        scenario: Option<String>,
        repo: RepoId,
    },
    /// Clicks a commit of the history, scrolled to first when it is below
    /// the window, until its file list and its first file's diff show.
    SelectCommit {
        scenario: Option<String>,
        hash: String,
    },
    /// Clicks a workspace tab, until its first paint: its tree.
    SwitchTab {
        scenario: Option<String>,
        tab: WorkspaceId,
    },
    /// From the last click, until the active tab's history, file list and
    /// diff show what they showed before (UI-31).
    HistoryShown { scenario: Option<String> },
    /// Scrolls the history from where it is to its last commit.
    ScrollHistory { scenario: String },
    /// Scrolls the diff from where it is to its end.
    ScrollDiff { scenario: String },
}

/// What a run measured, written for the driver.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    /// The display's refresh rate, when the platform tells it.
    pub refresh_hz: Option<f64>,
    pub samples: Vec<Sample>,
    pub scrolls: Vec<Scroll>,
    /// Frames scrolled while the window was not active, which GPUI slows
    /// down.
    pub inactive_frames: usize,
}

/// One timed action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub scenario: String,
    pub ms: f64,
}

/// One scroll run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scroll {
    pub scenario: String,
    /// The time between consecutive frames while scrolling.
    pub intervals_ms: Vec<f64>,
    /// The longest time a row on screen was still loading.
    pub longest_blank_ms: f64,
}

/// Where a run is after a tick.
#[derive(Debug)]
pub enum Tick {
    Running,
    Done(Trace),
    Failed(String),
}

/// What a timed action measures from.
#[derive(Debug, Clone, Copy)]
enum Since {
    Click(Instant),
    Process(SystemTime),
}

impl Since {
    fn elapsed_ms(self) -> f64 {
        let elapsed = match self {
            Since::Click(at) => at.elapsed(),
            Since::Process(at) => at.elapsed().unwrap_or_default(),
        };
        elapsed.as_secs_f64() * 1000.0
    }
}

enum Phase {
    /// The next step starts on this tick.
    Next,
    /// The pointer is over the element; the click follows on the next tick.
    Hovered(ElementId),
    /// Waiting for the action's result to show.
    Waiting(Option<Since>),
    /// The frame just drawn shows the result; its present is timed now.
    Shown(Option<Since>),
    Scrolling(ScrollRun),
}

struct ScrollRun {
    list: &'static str,
    last_frame: Option<Instant>,
    intervals_ms: Vec<f64>,
    blank_since: Option<Instant>,
    longest_blank: Duration,
    /// The diff's first row on screen, and for how many frames it stayed.
    top: Option<(usize, usize)>,
}

impl ScrollRun {
    fn new(list: &'static str) -> Self {
        Self {
            list,
            last_frame: None,
            intervals_ms: Vec::new(),
            blank_since: None,
            longest_blank: Duration::ZERO,
            top: None,
        }
    }
}

/// Plays a [`Script`] in a window.
pub struct Runner {
    window: Entity<GitcrocWindow>,
    started: Option<SystemTime>,
    steps: VecDeque<Step>,
    step: Option<Step>,
    step_began: Instant,
    /// When the last action clicked.
    clicked: Option<Instant>,
    phase: Phase,
    trace: Trace,
}

impl Runner {
    pub fn new(window: Entity<GitcrocWindow>, script: Script) -> Self {
        Self {
            window,
            started: script
                .started_ms
                .map(|ms| SystemTime::UNIX_EPOCH + Duration::from_millis(ms)),
            steps: script.steps.into(),
            step: None,
            step_began: Instant::now(),
            clicked: None,
            phase: Phase::Next,
            trace: Trace::default(),
        }
    }

    /// Runs one frame's part of the script; called at the start of each
    /// frame.
    pub fn tick(&mut self, window: &mut Window, cx: &mut App) -> Tick {
        if self.trace.refresh_hz.is_none() {
            self.trace.refresh_hz = window
                .refresh_interval(cx)
                .filter(|interval| !interval.is_zero())
                .map(|interval| 1.0 / interval.as_secs_f64());
        }
        if let Some(step) = &self.step
            && self.step_began.elapsed() > STEP_TIMEOUT
        {
            return Tick::Failed(format!(
                "{step:?} did not finish in {STEP_TIMEOUT:?}; {}",
                self.state(cx)
            ));
        }
        match self.advance(window, cx) {
            Ok(true) => Tick::Done(std::mem::take(&mut self.trace)),
            Ok(false) => Tick::Running,
            Err(error) => Tick::Failed(error),
        }
    }

    /// Moves the script on; `true` once every step is done.
    fn advance(&mut self, window: &mut Window, cx: &mut App) -> Result<bool, String> {
        match std::mem::replace(&mut self.phase, Phase::Next) {
            Phase::Next => {
                let Some(step) = self.steps.pop_front() else {
                    return Ok(true);
                };
                self.step = Some(step.clone());
                self.step_began = Instant::now();
                self.phase = self.start(&step, window, cx)?;
            }
            Phase::Hovered(id) => {
                click(window, &id, cx)?;
                let now = Instant::now();
                self.clicked = Some(now);
                let at = Since::Click(now);
                self.phase = Phase::Waiting(self.scenario().map(|_| at));
            }
            Phase::Waiting(since) => {
                self.phase = if self.shown(cx) {
                    Phase::Shown(since)
                } else {
                    Phase::Waiting(since)
                };
            }
            Phase::Shown(since) => {
                if let (Some(scenario), Some(since)) = (self.scenario(), since) {
                    self.trace.samples.push(Sample {
                        scenario,
                        ms: since.elapsed_ms(),
                    });
                }
                self.step = None;
            }
            Phase::Scrolling(run) => {
                self.phase = self.scroll(run, window, cx)?;
            }
        }
        Ok(false)
    }

    fn scenario(&self) -> Option<String> {
        match self.step.as_ref()? {
            Step::ColdStart { scenario }
            | Step::ScrollHistory { scenario }
            | Step::ScrollDiff { scenario } => Some(scenario.clone()),
            Step::SelectRepo { scenario, .. }
            | Step::SelectCommit { scenario, .. }
            | Step::SwitchTab { scenario, .. }
            | Step::HistoryShown { scenario } => scenario.clone(),
            Step::Idle => None,
        }
    }

    /// Starts `step` and returns what to do next.
    fn start(&mut self, step: &Step, window: &mut Window, cx: &mut App) -> Result<Phase, String> {
        Ok(match step {
            Step::ColdStart { .. } => {
                let started = self
                    .started
                    .ok_or("cold-start needs the time the process started")?;
                Phase::Waiting(Some(Since::Process(started)))
            }
            Step::Idle => Phase::Waiting(None),
            Step::HistoryShown { .. } => Phase::Waiting(self.clicked.map(Since::Click)),
            Step::SelectRepo { repo, .. } => {
                hover(window, &name(format!("tree-row-repo:{}", repo.0)), cx)?
            }
            Step::SwitchTab { tab, .. } => hover(window, &name(format!("tab-{}", tab.0)), cx)?,
            Step::SelectCommit { hash, .. } => {
                let index = self
                    .history_index(hash, cx)
                    .ok_or_else(|| format!("the history has no commit {hash}"))?;
                let row = name(format!("history-row-{index}"));
                if in_list(window, &row, "history-list") {
                    hover(window, &row, cx)?
                } else {
                    // Not timed: brings the row into the list first.
                    scroll_list(window, "history-list", cx)?;
                    self.steps.push_front(step.clone());
                    self.step = None;
                    Phase::Next
                }
            }
            Step::ScrollHistory { .. } => Phase::Scrolling(ScrollRun::new("history-list")),
            Step::ScrollDiff { .. } => Phase::Scrolling(ScrollRun::new("diff-lines")),
        })
    }

    /// Whether the window shows what the current step waits for.
    fn shown(&self, cx: &App) -> bool {
        let this = self.window.read(cx);
        match &self.step {
            Some(Step::ColdStart { .. }) => this.tree(cx).is_some_and(|tree| {
                let tree = tree.read(cx);
                let repos = tree.tree().repo_names();
                !tree.rows().is_empty()
                    && !repos.is_empty()
                    && repos.keys().all(|repo| {
                        tree.tree()
                            .summary(repo)
                            .is_some_and(|view| view.summary.is_some())
                    })
            }),
            Some(Step::Idle) => {
                let read = this.tree(cx).is_some_and(|tree| {
                    let tree = tree.read(cx).tree();
                    tree.repo_names().keys().all(|repo| {
                        tree.summary(repo)
                            .is_some_and(|view| !view.stale && !view.refreshing)
                    })
                });
                let history = this
                    .history(cx)
                    .is_none_or(|history| !history.read(cx).history().loading());
                read && history && self.changes_shown(cx)
            }
            Some(Step::SelectRepo { repo, .. }) => this.history(cx).is_some_and(|history| {
                let history = history.read(cx).history();
                history
                    .source()
                    .is_some_and(|source| source.key == format!("repo:{}", repo.0))
                    && !history.loading()
                    && !history.rows().is_empty()
            }),
            Some(Step::SelectCommit { hash, .. }) => {
                let selected = this.history(cx).and_then(|history| {
                    let history = history.read(cx).history();
                    history.selected().and_then(|id| history.commit_of(id))
                });
                selected.is_some_and(|(_, selected)| &selected == hash) && self.changes_shown(cx)
            }
            Some(Step::SwitchTab { tab, .. }) => {
                this.tabs(cx).active.as_ref() == Some(tab)
                    && this
                        .tree(cx)
                        .is_some_and(|tree| !tree.read(cx).rows().is_empty())
            }
            Some(Step::HistoryShown { .. }) => {
                let history = this.history(cx).is_some_and(|history| {
                    let history = history.read(cx).history();
                    history.source().is_none() || (!history.loading() && !history.rows().is_empty())
                });
                history && self.changes_shown(cx)
            }
            Some(Step::ScrollHistory { .. } | Step::ScrollDiff { .. }) | None => true,
        }
    }

    /// Whether the changes pane shows the history's selection and, when it
    /// has files, the selected file's diff.
    fn changes_shown(&self, cx: &App) -> bool {
        let this = self.window.read(cx);
        let selection = this
            .history(cx)
            .and_then(|history| history.read(cx).selection());
        let Some(changes) = this.changes(cx) else {
            return selection.is_none();
        };
        let pane = changes.read(cx);
        let changes = pane.changes();
        if selection.is_none() {
            return changes.selection().is_none();
        }
        if changes.selection() != selection.as_ref()
            || changes.view().is_none()
            || changes.loading()
        {
            return false;
        }
        match changes.diff_target() {
            None => true,
            Some(target) => {
                let diff = pane.diff().read(cx);
                diff.target() == Some(&target) && !diff.loading() && diff.content().is_some()
            }
        }
    }

    /// What the window shows, for a step that does not finish.
    fn state(&self, cx: &App) -> String {
        let this = self.window.read(cx);
        let history = this.history(cx).map(|history| {
            let history = history.read(cx).history();
            format!(
                "history: source {:?}, {} rows, loading {}, selected {:?}",
                history.source().map(|source| &source.key),
                history.rows().len(),
                history.loading(),
                history.selected()
            )
        });
        let changes = this.changes(cx).map(|changes| {
            let pane = changes.read(cx);
            let diff = pane.diff().read(cx);
            format!(
                "changes: selection {:?}, view {}, loading {}, diff of {:?}, diff shows {:?}, \
                 loading {}, content {}",
                pane.changes().selection(),
                pane.changes().view().is_some(),
                pane.changes().loading(),
                pane.changes().diff_target().map(|target| target.path),
                diff.target().map(|target| &target.path),
                diff.loading(),
                diff.content().is_some()
            )
        });
        format!(
            "active tab {:?}; {history:?}; {changes:?}",
            this.tabs(cx).active
        )
    }

    /// The place of commit `hash` among the active history's rows.
    fn history_index(&self, hash: &str, cx: &App) -> Option<usize> {
        let history = self.window.read(cx).history(cx)?;
        history
            .read(cx)
            .history()
            .rows()
            .iter()
            .position(|row| row.hash == hash)
    }

    /// One frame of a scroll run: records the frame, then scrolls on or
    /// ends.
    fn scroll(
        &mut self,
        mut run: ScrollRun,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Phase, String> {
        let now = Instant::now();
        if !window.is_window_active() {
            self.trace.inactive_frames += 1;
        }
        if let Some(last) = run.last_frame.replace(now) {
            run.intervals_ms
                .push(now.duration_since(last).as_secs_f64() * 1000.0);
        }
        // A row still loading in the frame just presented.
        if blank(window) {
            let since = *run.blank_since.get_or_insert(now);
            run.longest_blank = run.longest_blank.max(now.duration_since(since));
        } else {
            run.blank_since = None;
        }
        if self.scroll_ended(&mut run, window, cx) {
            self.trace.scrolls.push(Scroll {
                scenario: self.scenario().unwrap_or_default(),
                intervals_ms: run.intervals_ms,
                longest_blank_ms: run.longest_blank.as_secs_f64() * 1000.0,
            });
            self.step = None;
            return Ok(Phase::Next);
        }
        if run.intervals_ms.len() >= MAX_SCROLL_FRAMES {
            return Err(format!("{} did not reach its end", run.list));
        }
        scroll_list(window, run.list, cx)?;
        Ok(Phase::Scrolling(run))
    }

    fn scroll_ended(&self, run: &mut ScrollRun, window: &Window, cx: &App) -> bool {
        let this = self.window.read(cx);
        match self.step {
            Some(Step::ScrollHistory { .. }) => this.history(cx).is_some_and(|history| {
                let history = history.read(cx).history();
                let last = history.rows().len().checked_sub(1);
                history.done()
                    && last.is_some_and(|last| {
                        find_visible(window, &name(format!("history-row-{last}"))).is_some()
                    })
            }),
            Some(Step::ScrollDiff { .. }) => {
                let top = this
                    .changes(cx)
                    .map(|changes| changes.read(cx).diff().read(cx).top_row());
                let Some(top) = top else {
                    return true;
                };
                let still = match run.top {
                    Some((before, frames)) if before == top => frames + 1,
                    _ => 0,
                };
                run.top = Some((top, still));
                still >= STILL_FRAMES
            }
            _ => true,
        }
    }
}

fn name(text: String) -> ElementId {
    ElementId::Name(text.into())
}

/// Element `id` as the last frame drew it, when it is on screen.
fn find_visible(window: &Window, id: &ElementId) -> Option<ElementSnapshot> {
    observed::find_all(window, &[], id)
        .into_iter()
        .find(ElementSnapshot::visible)
}

/// Whether element `id` is drawn with its centre inside list `list`, where
/// a click reaches it.
fn in_list(window: &Window, id: &ElementId, list: &str) -> bool {
    let list = find_visible(window, &name(list.to_owned()));
    find_visible(window, id).is_some_and(|found| {
        list.is_some_and(|list| list.bounds().contains(&found.bounds().center()))
    })
}

fn require(window: &Window, id: &ElementId) -> Result<Point<gpui_kit::Pixels>, String> {
    find_visible(window, id)
        .map(|found| found.bounds().center())
        .ok_or_else(|| format!("{id:?} is not on screen"))
}

/// Moves the pointer over element `id`, which hovers it as a user's
/// pointer would before a click.
fn hover(window: &mut Window, id: &ElementId, cx: &mut App) -> Result<Phase, String> {
    let position = require(window, id)?;
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }),
        cx,
    );
    Ok(Phase::Hovered(id.clone()))
}

/// A left click at the centre of element `id`.
fn click(window: &mut Window, id: &ElementId, cx: &mut App) -> Result<(), String> {
    let position = require(window, id)?;
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }),
        cx,
    );
    Ok(())
}

/// One wheel step down over list `list`.
fn scroll_list(window: &mut Window, list: &str, cx: &mut App) -> Result<(), String> {
    let position = require(window, &name(list.to_owned()))?;
    window.dispatch_event(
        PlatformInput::Scroll(ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(-SCROLL_STEP))),
            ..Default::default()
        }),
        cx,
    );
    Ok(())
}

/// Whether a history row on screen is a placeholder.
fn blank(window: &Window) -> bool {
    observed::snapshots(window).iter().any(|found| {
        found.visible()
            && found.role() == Some(Role::ListBoxOption)
            && found.label() == Some(strings::history::LOADING)
    })
}
