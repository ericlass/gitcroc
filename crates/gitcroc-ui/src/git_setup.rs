//! The setup screen (PLAT-3, D13): shown instead of the main window while
//! no usable git is found. It says why, and offers to detect git again, to
//! choose it in a file dialog, or to give a chosen one up. Detection is the
//! app's (ARCH-6): its results reach the window as `GitStatus` events, and
//! the window passes them on here. Paths, versions and git's output are
//! drawn as text (SEC-3).

use gitcroc_core::GitStatus;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::{
    ClickEvent, Context, Div, ElementId, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    TestSupportExt, Window, div, px,
};

use crate::services::Services;
use crate::strings::git_setup as text;
use crate::theme::{ActiveTokens, AppTheme};

const MAX_WIDTH: f32 = 640.;

/// The setup screen over one result that is not usable.
pub struct GitSetup {
    services: Services,
    problem: GitStatus,
    /// `AppInfo::os`, for the install hint.
    os: String,
    /// A detection is running.
    checking: bool,
    /// Why giving the chosen git up failed, such as read-only settings.
    error: Option<SharedString>,
    focus: FocusHandle,
    first_button: FocusHandle,
}

impl GitSetup {
    pub fn new(services: Services, problem: GitStatus, os: String, cx: &mut Context<Self>) -> Self {
        Self {
            services,
            problem,
            os,
            checking: false,
            error: None,
            focus: cx.focus_handle(),
            first_button: cx.focus_handle(),
        }
    }

    /// The latest result that is not usable.
    pub fn set_problem(&mut self, problem: GitStatus, cx: &mut Context<Self>) {
        self.problem = problem;
        cx.notify();
    }

    /// Whether a detection is running.
    pub fn set_checking(&mut self, checking: bool, cx: &mut Context<Self>) {
        self.checking = checking;
        cx.notify();
    }

    /// Gives the focus to the first button.
    pub fn focus_first(&self, window: &mut Window, cx: &mut gpui_kit::App) {
        window.focus(&self.first_button, cx);
    }

    fn check_again(&mut self, _: &ClickEvent, _: &mut Window, _: &mut Context<Self>) {
        // It runs on even though its result is not awaited: that arrives as
        // events.
        let _detecting = self
            .services
            .with_app(|app| async move { app.git_recheck() });
    }

    fn choose(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        let _choosing = self
            .services
            .with_app(|app| async move { app.git_pick_path() });
    }

    fn use_automatic(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let call = self
            .services
            .with_app(|app| async move { app.settings_set_git_path(None).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                this.error = match result {
                    Some(Err(error)) => Some(error.message.into()),
                    _ => None,
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The problem's lines: an alert, then what the user can check.
    fn problem_lines(&self, cx: &Context<Self>) -> Vec<Stateful<Div>> {
        match &self.problem {
            GitStatus::NotFound { searched, .. } if searched.is_empty() => vec![
                line("git-setup-problem", Role::Alert, text::NOT_FOUND),
                line("git-setup-searched", Role::Paragraph, text::NOWHERE_TO_LOOK),
            ],
            GitStatus::NotFound { searched, .. } => vec![
                line("git-setup-problem", Role::Alert, text::NOT_FOUND),
                line("git-setup-searched", Role::Paragraph, text::LOOKED_AT),
                div()
                    .id("git-setup-searched-list")
                    .role(Role::List)
                    .pl_4()
                    .children(searched.iter().enumerate().map(|(at, path)| {
                        line(("git-setup-path", at), Role::ListItem, path.clone()).test_support()
                    })),
            ],
            GitStatus::TooOld {
                path,
                found,
                required,
                ..
            } => vec![line(
                "git-setup-problem",
                Role::Alert,
                text::too_old(found, path, required),
            )],
            GitStatus::Unparseable { path, output, .. } => {
                let tokens = cx.tokens();
                vec![
                    line("git-setup-problem", Role::Alert, text::unparseable(path)),
                    line("git-setup-output", Role::Paragraph, output.clone())
                        .font_family(AppTheme::global(cx).code_font().clone())
                        .p_2()
                        .rounded(px(4.))
                        .bg(tokens.bg_2)
                        .border_1()
                        .border_color(tokens.border),
                ]
            }
            // The window shows this screen only for the results above.
            GitStatus::Checking | GitStatus::Usable { .. } => Vec::new(),
        }
    }
}

/// The git chosen in Settings, and whether it was not found.
fn chosen(status: &GitStatus) -> (Option<&str>, bool) {
    match status {
        GitStatus::Checking => (None, false),
        GitStatus::Usable {
            chosen,
            chosen_missing,
            ..
        }
        | GitStatus::NotFound {
            chosen,
            chosen_missing,
            ..
        }
        | GitStatus::TooOld {
            chosen,
            chosen_missing,
            ..
        }
        | GitStatus::Unparseable {
            chosen,
            chosen_missing,
            ..
        } => (chosen.as_deref(), *chosen_missing),
    }
}

/// A line of text, named by its text for tests and screen readers.
fn line(id: impl Into<ElementId>, role: Role, text: impl Into<SharedString>) -> Stateful<Div> {
    let text = text.into();
    div().id(id).role(role).aria_label(text.clone()).child(text)
}

impl Focusable for GitSetup {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for GitSetup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *cx.tokens();
        let (chosen, chosen_missing) = chosen(&self.problem);
        let missing = chosen.filter(|_| chosen_missing).map(|chosen| {
            line(
                "git-setup-chosen-missing",
                Role::Paragraph,
                text::chosen_missing(chosen),
            )
            .test_support()
        });
        let error = self.error.clone().map(|error| {
            line("git-setup-error", Role::Alert, error)
                .text_color(tokens.danger)
                .test_support()
        });
        let buttons = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new("git-setup-check-again")
                    .primary()
                    .label(if self.checking {
                        text::CHECKING
                    } else {
                        text::CHECK_AGAIN
                    })
                    .disabled(self.checking)
                    .track_focus(&self.first_button)
                    .on_click(cx.listener(Self::check_again)),
            )
            .child(
                Button::new("git-setup-choose")
                    .label(text::CHOOSE_GIT)
                    .disabled(self.checking)
                    .on_click(cx.listener(Self::choose)),
            )
            .children(chosen.is_some().then(|| {
                Button::new("git-setup-automatic")
                    .label(text::USE_AUTOMATIC)
                    .disabled(self.checking)
                    .on_click(cx.listener(Self::use_automatic))
            }));
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .max_w(px(MAX_WIDTH))
            .child(
                line("git-setup-heading", Role::Heading, text::TITLE)
                    .aria_level(1)
                    .text_xl()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .test_support(),
            )
            .children(
                self.problem_lines(cx)
                    .into_iter()
                    .map(TestSupportExt::test_support),
            )
            .children(missing)
            .child(
                line(
                    "git-setup-hint",
                    Role::Paragraph,
                    text::install_hint(&self.os),
                )
                .text_color(tokens.text_secondary)
                .test_support(),
            )
            .children(error)
            .child(buttons);
        // A long search list scrolls, as on Windows.
        div()
            .id("git-setup")
            .test_support()
            .track_focus(&self.focus)
            .role(Role::Main)
            .aria_label(text::TITLE)
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .p_8()
            .bg(tokens.bg_1)
            .text_color(tokens.text_primary)
            .child(content)
    }
}
