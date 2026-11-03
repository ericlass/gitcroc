//! Resolve all with AI from the conflict banner (AI-3, AI-13), a port of
//! rc.1's `stores/resolveAll.ts` and `conflict/AiFileList.tsx`. One model
//! per workspace keeps each repo's run (ARCH-9): the request on its way,
//! why it was refused, and the job it started, whose view, as the job list
//! hears it, carries each file's progress. The banner shows it; the changes
//! pane, which has the repo's unmerged list and its kept editors (OPS-9),
//! sends the request when asked. The core decides what is sent and runs
//! the files (ARCH-6); a run goes on in a background tab (UI-30). Nothing is
//! accepted for the user (D9). With AI features off nothing of it shows,
//! and every run is forgotten (AI-16). Paths and reasons are text only
//! (SEC-3).

use std::collections::HashMap;
use std::rc::Rc;

use gitcroc_core::{AiFileState, JobId, JobState, JobView, RepoId, WorkspaceId};
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::{
    App, Context, ElementId, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Role, SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window,
    div,
};

use crate::ai::Ai;
use crate::jobs::Jobs;
use crate::services::Services;
use crate::strings::conflict as text;
use crate::theme::tokens::Tokens;
use crate::workspaces::Workspaces;

/// What the banner asks of the changes pane.
#[derive(Debug, Clone)]
pub enum ResolveAllAsk {
    /// Send Resolve all with AI for `repo`.
    Start { repo: RepoId },
    /// Open `path` of `repo` in the conflict editor, with its proposal.
    Open { repo: RepoId, path: String },
}

/// A repo's Resolve all.
#[derive(Debug, Default)]
pub struct RepoRun {
    /// A request is on its way, or waits for the repo's lists.
    starting: bool,
    /// Why the last request was refused or did not go out.
    refusal: Option<String>,
    /// The job the last request started.
    job: Option<JobId>,
    /// That job's view, as the job list last heard it.
    view: Option<JobView>,
}

impl RepoRun {
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    pub fn view(&self) -> Option<&JobView> {
        self.view.as_ref()
    }

    /// Its job runs or waits.
    pub fn running(&self) -> bool {
        self.job.is_some()
            && self
                .view
                .as_ref()
                .is_none_or(|view| view.state != JobState::Ended)
    }

    /// A request is on its way or the job runs: Resolve all is Cancel.
    pub fn busy(&self) -> bool {
        self.starting || self.running()
    }
}

/// The Resolve all runs of one workspace.
pub struct ResolveAll {
    services: Services,
    workspace_id: WorkspaceId,
    ai: Entity<Ai>,
    workspaces: Entity<Workspaces>,
    jobs: Entity<Jobs>,
    runs: HashMap<RepoId, RepoRun>,
    _ai_watch: Subscription,
    _workspaces_watch: Subscription,
    _jobs_watch: Subscription,
}

impl EventEmitter<ResolveAllAsk> for ResolveAll {}

impl ResolveAll {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        ai: Entity<Ai>,
        workspaces: Entity<Workspaces>,
        jobs: Entity<Jobs>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Off ends every run in the core and drops its proposals (AI-16).
        let ai_watch = cx.observe(&ai, |this, ai, cx| {
            if !ai.read(cx).enabled() {
                this.runs.clear();
            }
            cx.notify();
        });
        let workspaces_watch = cx.observe(&workspaces, |_, _, cx| cx.notify());
        let jobs_watch = cx.observe(&jobs, |this, _, cx| this.follow_jobs(cx));
        Self {
            services,
            workspace_id,
            ai,
            workspaces,
            jobs,
            runs: HashMap::new(),
            _ai_watch: ai_watch,
            _workspaces_watch: workspaces_watch,
            _jobs_watch: jobs_watch,
        }
    }

    /// AI features (AI-16).
    pub fn enabled(&self, cx: &App) -> bool {
        self.ai.read(cx).enabled()
    }

    /// AI for this workspace (AI-6).
    pub fn workspace_on(&self, cx: &App) -> bool {
        self.workspaces
            .read(cx)
            .view(&self.workspace_id)
            .is_none_or(|view| view.ai)
    }

    pub fn run(&self, repo: &RepoId) -> Option<&RepoRun> {
        self.runs.get(repo)
    }

    /// Resolve all with AI was pressed for `repo` (AI-3).
    pub fn ask(&mut self, repo: &RepoId, cx: &mut Context<Self>) {
        if !self.enabled(cx)
            || !self.workspace_on(cx)
            || self.runs.get(repo).is_some_and(RepoRun::busy)
        {
            return;
        }
        cx.emit(ResolveAllAsk::Start { repo: repo.clone() });
    }

    /// A file's name in the progress was pressed (AI-13).
    pub fn open(&mut self, repo: &RepoId, path: String, cx: &mut Context<Self>) {
        cx.emit(ResolveAllAsk::Open {
            repo: repo.clone(),
            path,
        });
    }

    /// Cancels `repo`'s run: waiting files never go and running requests
    /// are dropped; what finished stays (AI-13). Its end arrives as
    /// `JobEnded`.
    pub fn cancel(&mut self, repo: &RepoId, cx: &mut Context<Self>) {
        let Some(job) = self
            .runs
            .get(repo)
            .filter(|run| run.running())
            .and_then(|run| run.job)
        else {
            return;
        };
        drop(
            self.services
                .with_app(move |app| async move { app.job_cancel(job) }),
        );
        cx.notify();
    }

    /// The request for `repo` is on its way, or waits for its lists.
    pub fn starting(&mut self, repo: &RepoId, cx: &mut Context<Self>) {
        let run = self.runs.entry(repo.clone()).or_default();
        run.starting = true;
        run.refusal = None;
        cx.notify();
    }

    /// The request for `repo` started `job`.
    pub fn started(&mut self, repo: &RepoId, job: JobId, cx: &mut Context<Self>) {
        let view = self.jobs.read(cx).get(job).cloned();
        let run = self.runs.entry(repo.clone()).or_default();
        run.starting = false;
        run.job = Some(job);
        run.view = view;
        cx.notify();
    }

    /// The request for `repo` started nothing, with `refusal` to say why.
    pub fn settled(&mut self, repo: &RepoId, refusal: Option<String>, cx: &mut Context<Self>) {
        let run = self.runs.entry(repo.clone()).or_default();
        run.starting = false;
        run.refusal = refusal;
        cx.notify();
    }

    /// Keeps each run's view as the job list hears it; notifies only when
    /// one changed.
    fn follow_jobs(&mut self, cx: &mut Context<Self>) {
        let jobs = self.jobs.read(cx);
        let mut changed = false;
        for run in self.runs.values_mut() {
            let Some(job) = run.job else {
                continue;
            };
            let Some(view) = jobs.get(job) else {
                continue;
            };
            if run.view.as_ref() != Some(view) {
                run.view = Some(view.clone());
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }
}

/// Where a file is, in words (AI-13).
pub fn file_state_text(state: &AiFileState) -> String {
    match state {
        AiFileState::Waiting => text::WAITING.to_owned(),
        AiFileState::Running => text::RUNNING.to_owned(),
        AiFileState::Done {
            high,
            medium,
            low,
            unresolved,
        } => text::done_with(*high, *medium, *low, *unresolved),
        AiFileState::Failed { reason } => text::ai_failed(reason),
        AiFileState::Ineligible { message } => text::not_sent(message),
        AiFileState::AllEdited => text::ALL_EDITED.to_owned(),
        AiFileState::Cancelled => text::CANCELLED.to_owned(),
    }
}

/// Opens a file of the list by its path.
pub type OpenFile = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// The element ids of a file list: the list, its rows, each row's file name and
/// each row's state.
#[derive(Debug, Clone, Copy)]
pub struct ListIds {
    pub list: &'static str,
    pub row: &'static str,
    pub file: &'static str,
    pub state: &'static str,
}

/// The files of `job` and where each is, as the core reports them (AI-13).
/// With `open`, each name is a button that opens the file.
pub fn file_list(
    ids: ListIds,
    job: &JobView,
    open: Option<OpenFile>,
    tokens: &Tokens,
    cx: &App,
) -> impl IntoElement + use<> {
    let rows = job.ai_files.iter().enumerate().map(|(at, file)| {
        let path: SharedString = file.path.clone().into();
        let state: SharedString = file_state_text(&file.state).into();
        let name = match &open {
            // A button, so Tab reaches it and Space or Enter opens the file
            // (UI-22).
            Some(open) => {
                let (open, opened) = (open.clone(), file.path.clone());
                Button::new(ElementId::NamedInteger(ids.file.into(), at as u64))
                    .link()
                    .small()
                    .label(path.clone())
                    .on_click(move |_, window, cx| open(opened.clone(), window, cx))
                    .into_any_element()
            }
            None => div()
                .id(ElementId::NamedInteger(ids.file.into(), at as u64))
                .test_support()
                .role(Role::Label)
                .aria_label(path.clone())
                .child(path.clone())
                .into_any_element(),
        };
        let shown = match file.state {
            AiFileState::Running => div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(crate::job_list::spinner(cx))
                .child(state.clone()),
            _ => div().child(state.clone()),
        };
        div()
            .id(ElementId::NamedInteger(ids.row.into(), at as u64))
            .test_support()
            .role(Role::ListItem)
            .aria_label(SharedString::from(text::file_state(&path, &state)))
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .min_w_0()
            .child(name)
            .child(
                div()
                    .id(ElementId::NamedInteger(ids.state.into(), at as u64))
                    .test_support()
                    .role(Role::Status)
                    .aria_label(state.clone())
                    .min_w_0()
                    .text_color(match file.state {
                        AiFileState::Failed { .. } => tokens.danger,
                        _ => tokens.text_secondary,
                    })
                    .child(shown),
            )
    });
    div()
        .id(ids.list)
        .test_support()
        .role(Role::List)
        .aria_label(text::FILES)
        .flex()
        .flex_col()
        .gap_1()
        .children(rows)
}
