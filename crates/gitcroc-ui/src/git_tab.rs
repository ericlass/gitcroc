//! The Git tab of Settings, a port of the second panel of
//! `ui/src/SettingsDialog.tsx`: the git in use, its version and path, and
//! when the chosen git is missing, what is used instead (PLAT-3); Choose
//! git… and Use automatic detection, as on the setup screen; and how group
//! operations run (section 5): how many repos at once, and whether the
//! preflight dialog always shows. The window passes git detection's results
//! on; the tab keeps no copy of the settings (ARCH-6). Paths are text
//! (SEC-3).

use gitcroc_core::GitStatus;
use gpui_component::Disableable;
use gpui_component::button::Button;
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Task, TestSupportExt, Window, div, px,
};

use crate::services::Services;
use crate::settings::SettingsModel;
use crate::strings::{git_setup, settings as text};
use crate::theme::ActiveTokens;

pub struct GitTab {
    services: Services,
    settings: Entity<SettingsModel>,
    /// The latest finished detection, as the window knows it.
    git: Option<GitStatus>,
    /// A detection is running.
    checking: bool,
    parallelism: Entity<InputState>,
    /// The parallelism was typed into and not saved yet.
    edited: bool,
    git_error: Option<SharedString>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl GitTab {
    pub fn new(
        services: Services,
        settings: Entity<SettingsModel>,
        git: Option<GitStatus>,
        checking: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let saved = settings.read(cx).view().settings.groups.group_parallelism;
        let parallelism = cx.new(|cx| InputState::new(window, cx).default_value(saved.to_string()));
        let subscriptions = vec![
            cx.observe_in(&settings, window, |this, _, window, cx| {
                this.sync(window, cx);
                cx.notify();
            }),
            cx.subscribe_in(
                &parallelism,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.edited = true,
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        this.save_parallelism(window, cx)
                    }
                    InputEvent::Focus => {}
                },
            ),
        ];
        Self {
            services,
            settings,
            git,
            checking,
            parallelism,
            edited: false,
            git_error: None,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Git detection's start or result, as the window heard it.
    pub fn set_git(&mut self, git: Option<GitStatus>, checking: bool, cx: &mut Context<Self>) {
        self.git = git;
        self.checking = checking;
        cx.notify();
    }

    /// Shows the saved parallelism, unless it is being typed.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edited {
            return;
        }
        let saved = self
            .settings
            .read(cx)
            .view()
            .settings
            .groups
            .group_parallelism
            .to_string();
        if self.parallelism.read(cx).value() != saved {
            self.parallelism
                .update(cx, |state, cx| state.set_value(saved, window, cx));
            self.edited = false;
        }
    }

    fn save_parallelism(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.edited {
            return;
        }
        self.edited = false;
        // Text that is not a number puts the saved value back; the core
        // refuses one outside its limits.
        let Ok(parallelism) = self.parallelism.read(cx).value().trim().parse::<u32>() else {
            self.sync(window, cx);
            return;
        };
        let request = self.settings.update(cx, |settings, cx| {
            settings.set_groups(move |groups| groups.group_parallelism = parallelism, cx)
        });
        self.send(request, cx);
    }

    fn set_always_show_preflight(&mut self, on: bool, cx: &mut Context<Self>) {
        let request = self.settings.update(cx, |settings, cx| {
            settings.set_groups(move |groups| groups.always_show_preflight = on, cx)
        });
        self.send(request, cx);
    }

    fn send(&mut self, request: Task<Result<(), SharedString>>, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            if let Err(error) = request.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn choose(&mut self, cx: &mut Context<Self>) {
        self.git_error = None;
        cx.notify();
        // Its result arrives as `GitStatus` events.
        let _choosing = self
            .services
            .with_app(|app| async move { app.git_pick_path() });
    }

    fn use_automatic(&mut self, cx: &mut Context<Self>) {
        self.git_error = None;
        let call = self
            .services
            .with_app(|app| async move { app.settings_set_git_path(None).await });
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Err(error)) = result {
                    this.git_error = Some(error.message.into());
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn git_section(&self, read_only: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut section = group("settings-git", text::GIT_SECTION);
        let chosen = match &self.git {
            Some(GitStatus::Usable {
                path,
                version,
                chosen,
                chosen_missing,
            }) => {
                section = section.child(line(
                    "settings-git-in-use",
                    Role::Paragraph,
                    text::git(version, path),
                ));
                if *chosen_missing && let Some(chosen) = chosen {
                    section = section.child(line(
                        "settings-git-missing",
                        Role::Alert,
                        text::chosen_missing(chosen, path),
                    ));
                }
                chosen.is_some()
            }
            _ => false,
        };
        if let Some(error) = self.git_error.clone() {
            section = section.child(alert("settings-git-error", error, cx));
        }
        let busy = read_only || self.checking;
        section
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        Button::new("settings-choose-git")
                            .label(git_setup::CHOOSE_GIT)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.choose(cx))),
                    )
                    .when(chosen, |this| {
                        this.child(
                            Button::new("settings-git-automatic")
                                .label(git_setup::USE_AUTOMATIC)
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.use_automatic(cx))),
                        )
                    }),
            )
            .into_any_element()
    }

    fn group_section(&self, read_only: bool, cx: &mut Context<Self>) -> AnyElement {
        let always = self
            .settings
            .read(cx)
            .view()
            .settings
            .groups
            .always_show_preflight;
        group("settings-groups", text::GROUP_OPERATIONS)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(text::REPOS_AT_ONCE)
                    .child(
                        div().w(px(64.)).child(
                            Input::new(&self.parallelism)
                                .id("settings-repos-at-once")
                                .aria_label(text::REPOS_AT_ONCE)
                                .disabled(read_only),
                        ),
                    ),
            )
            .child(
                Checkbox::new("settings-always-preflight")
                    .label(text::ALWAYS_SHOW_PREFLIGHT)
                    .checked(always)
                    .disabled(read_only)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.set_always_show_preflight(*value, cx);
                    })),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(alert("settings-groups-error", error, cx))
            })
            .into_any_element()
    }
}

fn group(
    id: &'static str,
    legend: &'static str,
) -> impl ParentElement + IntoElement + Styled + use<> {
    div()
        .id(id)
        .test_support()
        .role(Role::Group)
        .aria_label(legend)
        .flex()
        .flex_col()
        .gap_2()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(legend))
}

fn line(id: &'static str, role: Role, said: impl Into<SharedString>) -> AnyElement {
    let said = said.into();
    div()
        .id(id)
        .test_support()
        .role(role)
        .aria_label(said.clone())
        .child(said)
        .into_any_element()
}

fn alert(id: &'static str, error: SharedString, cx: &gpui_kit::App) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .role(Role::Alert)
        .aria_label(error.clone())
        .text_color(cx.tokens().danger)
        .child(error)
        .into_any_element()
}

impl Render for GitTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let read_only = self.settings.read(cx).read_only();
        div()
            .id("settings-git-tab")
            .test_support()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.git_section(read_only, cx))
            .child(self.group_section(read_only, cx))
    }
}
