//! The tree's add menu and add report (GRP-1), a port of the add menu and
//! `AddReportView` of rc.1's `ui/src/tree/TreePane.tsx`. A button beside
//! the filter box opens a menu with Add folder… and Clone…, with the mouse
//! or, focused, with Enter or Space, as the other menus do (UI-22). Add
//! folder asks the core to open its folder dialog and run the add job
//! (SEC-2, ARCH-7); the job's report shows below the tree's header until
//! its close button or the next add. Clone… asks the window for the clone
//! dialog, with the parent folder last picked in this workspace. The core
//! refuses what it refuses, such as a read-only workspace, and the refusal
//! shows where the report does. One of each per workspace (ARCH-9).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use gitcroc_app::app::AppEvent;
use gitcroc_core::{AddReport, RepoView, WorkspaceId};
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::{
    Anchor, App, AppContext, Bounds, ClickEvent, Context, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Pixels, Render, Role,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window, canvas,
    div,
};

use crate::menu::{Menu, MenuEvent, MenuItem};
use crate::services::Services;
use crate::strings::add_repos as text;
use crate::theme::ActiveTokens;

/// The user chose Clone…: the window opens the clone dialog for
/// `workspace`, starting with `parent`, the folder last picked there.
#[derive(Debug, Clone)]
pub struct AskClone {
    pub workspace: WorkspaceId,
    pub parent: Option<PathBuf>,
}

/// The entries of the add menu, in order.
const ADD_FOLDER: usize = 0;
const CLONE: usize = 1;

/// The add button and its menu, for one workspace.
pub struct AddMenu {
    services: Services,
    workspace_id: WorkspaceId,
    report: Entity<AddReportView>,
    /// The clone dialog's parent folder last picked, for the next clone.
    parent: Option<PathBuf>,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    menu: Option<(Entity<Menu>, Subscription)>,
}

impl EventEmitter<AskClone> for AddMenu {}

impl AddMenu {
    pub fn new(
        services: Services,
        workspace_id: WorkspaceId,
        report: Entity<AddReportView>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            services,
            workspace_id,
            report,
            parent: None,
            focus: cx.focus_handle(),
            bounds: Rc::default(),
            menu: None,
        }
    }

    /// Whether the menu is open.
    pub fn is_open(&self) -> bool {
        self.menu.is_some()
    }

    /// Follows the add reports and the clone dialog's folder picks of this
    /// workspace.
    pub fn on_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::WorkspaceAddReport(report) if report.workspace_id == self.workspace_id => {
                let report = report.clone();
                self.report
                    .update(cx, |view, cx| view.set_report(report, cx));
            }
            AppEvent::CloneParentPicked {
                workspace_id,
                path,
                key: None,
            } if *workspace_id == self.workspace_id => self.parent = Some(path.clone()),
            _ => {}
        }
    }

    fn open(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let items = vec![MenuItem::new(text::ADD_FOLDER), MenuItem::new(text::CLONE)];
        let at = self.bounds.get().bottom_left();
        let menu = cx.new(|cx| Menu::new(text::ADD_REPOSITORY, items, at, Anchor::TopLeft, cx));
        let events = cx.subscribe_in(&menu, window, |this, _, event: &MenuEvent, window, cx| {
            this.on_menu(*event, window, cx);
        });
        menu.update(cx, |menu, cx| menu.focus_first(window, cx));
        self.menu = Some((menu, events));
        cx.notify();
    }

    fn on_menu(&mut self, event: MenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_none() {
            return;
        }
        // The focus goes back to the button, before a dialog an entry
        // opens takes it.
        window.focus(&self.focus, cx);
        cx.notify();
        match event {
            MenuEvent::Chosen(ADD_FOLDER) => self.add_folder(cx),
            MenuEvent::Chosen(CLONE) => {
                self.report.update(cx, |view, cx| view.clear_refusal(cx));
                cx.emit(AskClone {
                    workspace: self.workspace_id.clone(),
                    parent: self.parent.clone(),
                });
            }
            MenuEvent::Chosen(_) | MenuEvent::Dismissed => {}
        }
    }

    /// Asks the core for the folder dialog and the add job; the next
    /// report replaces this one, and a cancel shows none.
    fn add_folder(&mut self, cx: &mut Context<Self>) {
        self.report.update(cx, |view, cx| view.clear(cx));
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(|app| async move { app.workspace_pick_and_add(workspace).await });
        let report = self.report.downgrade();
        cx.spawn(async move |_, cx| {
            if let Some(Err(error)) = call.await {
                let _ = report.update(cx, |view, cx| view.set_refusal(error.message.into(), cx));
            }
        })
        .detach();
    }
}

impl Focusable for AddMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AddMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.bounds.clone();
        div()
            .relative()
            .flex_shrink_0()
            .child(
                canvas(move |drawn, _, _| bounds.set(drawn), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                Button::new("tree-add")
                    .ghost()
                    .compact()
                    .label("+")
                    .accessibility_label(text::ADD_REPOSITORY)
                    .tooltip(text::ADD_REPOSITORY)
                    .track_focus(&self.focus)
                    .on_click(cx.listener(Self::open)),
            )
            .children(self.menu.as_ref().map(|(menu, _)| menu.clone()))
    }
}

/// What the last add of a workspace did, or why the core refused it.
#[derive(Default)]
pub struct AddReportView {
    report: Option<AddReport>,
    refusal: Option<SharedString>,
}

impl AddReportView {
    pub fn report(&self) -> Option<&AddReport> {
        self.report.as_ref()
    }

    fn set_report(&mut self, report: AddReport, cx: &mut Context<Self>) {
        self.report = Some(report);
        self.refusal = None;
        cx.notify();
    }

    fn set_refusal(&mut self, refusal: SharedString, cx: &mut Context<Self>) {
        self.refusal = Some(refusal);
        cx.notify();
    }

    fn clear_refusal(&mut self, cx: &mut Context<Self>) {
        if self.refusal.take().is_some() {
            cx.notify();
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.report = None;
        self.refusal = None;
        cx.notify();
    }

    /// The report's lines, as rc.1 listed them, each with whether it
    /// says a folder was refused.
    fn lines(report: &AddReport) -> Vec<(String, bool)> {
        let names = |repos: &[RepoView]| {
            repos
                .iter()
                .map(|repo| repo.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut lines = Vec::new();
        if !report.added.is_empty() {
            lines.push((text::added(&names(&report.added)), false));
        }
        if !report.already_present.is_empty() {
            lines.push((
                text::already_present(&names(&report.already_present)),
                false,
            ));
        }
        for refusal in &report.refused {
            lines.push((text::refused(&refusal.path, &refusal.reason), true));
        }
        if lines.is_empty() {
            lines.push((text::NO_REPOS_FOUND.to_owned(), false));
        }
        lines
    }
}

impl Render for AddReportView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let refusal = self.refusal.clone().map(|refusal| {
            div()
                .id("tree-add-refusal")
                .test_support()
                .role(Role::Alert)
                .aria_label(refusal.clone())
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .px_2()
                .py_1()
                .text_color(tokens.danger)
                .child(div().flex_1().min_w_0().child(refusal))
                .child(
                    Button::new("tree-add-refusal-close")
                        .ghost()
                        .compact()
                        .label(text::CLOSE)
                        .on_click(cx.listener(|this, _, _, cx| this.clear_refusal(cx))),
                )
        });
        let report = self.report.as_ref().map(|report| {
            let lines = Self::lines(report)
                .into_iter()
                .enumerate()
                .map(|(at, (line, refused))| {
                    div()
                        .id(ElementId::NamedInteger("tree-add-line".into(), at as u64))
                        .test_support()
                        .role(Role::ListItem)
                        .aria_label(line.clone())
                        .text_color(if refused {
                            tokens.danger
                        } else {
                            tokens.text_primary
                        })
                        .child(line)
                });
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(tokens.border)
                .child(
                    div()
                        .id("tree-add-report")
                        .test_support()
                        .role(Role::List)
                        .aria_label(text::ADD_RESULT)
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .children(lines),
                )
                .child(
                    Button::new("tree-add-report-close")
                        .ghost()
                        .compact()
                        .label(text::CLOSE)
                        .accessibility_label(text::CLOSE_REPORT)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.report = None;
                            cx.notify();
                        })),
                )
        });
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .children(refusal)
            .children(report)
    }
}
