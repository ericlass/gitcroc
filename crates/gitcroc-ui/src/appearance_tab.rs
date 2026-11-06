//! The Appearance tab of Settings, a port of the first panel of
//! rc.1's `ui/src/SettingsDialog.tsx`: the app theme (UI-23), a code theme for
//! light mode and one for dark mode, each with its own preview (UI-24), the
//! UI and code fonts with their sizes (UI-26), and the system title bar
//! (UI-35). Every change goes to the core, and the window applies the view
//! it returns. Where the window reopens to change its title bar, unsaved
//! conflict edits are asked about first (OPS-9).

use std::rc::Rc;

use gitcroc_core::{Appearance, CodeTheme, Theme};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Disableable, Selectable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, FontWeight, HighlightStyle, InteractiveElement,
    IntoElement, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    StyledText, Subscription, Task, TestSupportExt, Window, div, px,
};

use crate::settings::SettingsModel;
use crate::strings::{code_themes as theme_names, common, settings as text};
use crate::theme::ActiveTokens;
use crate::theme::code_themes::{CODE_THEMES, Syntax, code_theme};

/// The files with unsaved conflict edits in every tab, which reopening the
/// window loses (OPS-9).
pub type Unsaved = Rc<dyn Fn(&App) -> Vec<String>>;

/// The themes in the order the tab lists them.
const THEMES: [(Theme, &str); 3] = [
    (Theme::System, text::THEME_SYSTEM),
    (Theme::Light, text::THEME_LIGHT),
    (Theme::Dark, text::THEME_DARK),
];

/// A code theme's name as the tab lists it.
pub fn code_theme_label(theme: CodeTheme) -> &'static str {
    match theme {
        CodeTheme::GithubLight => theme_names::GITHUB_LIGHT,
        CodeTheme::GithubDark => theme_names::GITHUB_DARK,
        CodeTheme::SolarizedLight => theme_names::SOLARIZED_LIGHT,
        CodeTheme::SolarizedDark => theme_names::SOLARIZED_DARK,
        CodeTheme::OneDark => theme_names::ONE_DARK,
        CodeTheme::Dracula => theme_names::DRACULA,
        CodeTheme::Nord => theme_names::NORD,
        CodeTheme::Monokai => theme_names::MONOKAI,
    }
}

/// Which code theme choice: the one for light mode or for dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Light,
    Dark,
}

impl Mode {
    fn id(self) -> &'static str {
        match self {
            Self::Light => "settings-code-theme-light",
            Self::Dark => "settings-code-theme-dark",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Light => text::CODE_THEME_LIGHT,
            Self::Dark => text::CODE_THEME_DARK,
        }
    }

    fn of(self, appearance: &Appearance) -> CodeTheme {
        match self {
            Self::Light => appearance.code_theme_light,
            Self::Dark => appearance.code_theme_dark,
        }
    }

    fn set(self, appearance: &mut Appearance, theme: CodeTheme) {
        match self {
            Self::Light => appearance.code_theme_light = theme,
            Self::Dark => appearance.code_theme_dark = theme,
        }
    }
}

/// A text field of the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    UiFont,
    UiSize,
    CodeFont,
    CodeSize,
}

impl FieldKind {
    const ALL: [Self; 4] = [Self::UiFont, Self::UiSize, Self::CodeFont, Self::CodeSize];

    fn id(self) -> &'static str {
        match self {
            Self::UiFont => "settings-ui-font",
            Self::UiSize => "settings-ui-font-size",
            Self::CodeFont => "settings-code-font",
            Self::CodeSize => "settings-code-font-size",
        }
    }

    /// The saved value, as the field shows it.
    fn saved(self, appearance: &Appearance) -> String {
        match self {
            Self::UiFont => appearance.ui_font.clone().unwrap_or_default(),
            Self::UiSize => appearance.ui_font_size.to_string(),
            Self::CodeFont => appearance.code_font.clone().unwrap_or_default(),
            Self::CodeSize => appearance.code_font_size.to_string(),
        }
    }

    /// Puts `typed` into `appearance`; `false` for a size that is not a
    /// number, which puts the saved one back.
    fn apply(self, typed: &str, appearance: &mut Appearance) -> bool {
        let font = || Some(typed.to_owned()).filter(|name| !name.trim().is_empty());
        let size = || typed.trim().parse::<u32>().ok();
        match self {
            Self::UiFont => appearance.ui_font = font(),
            Self::CodeFont => appearance.code_font = font(),
            Self::UiSize => match size() {
                Some(size) => appearance.ui_font_size = size,
                None => return false,
            },
            Self::CodeSize => match size() {
                Some(size) => appearance.code_font_size = size,
                None => return false,
            },
        }
        true
    }
}

struct Field {
    kind: FieldKind,
    state: Entity<InputState>,
    /// Typed into and not saved yet.
    edited: bool,
}

pub struct AppearanceTab {
    settings: Entity<SettingsModel>,
    unsaved: Unsaved,
    /// The window changes its title bar without reopening (Linux).
    in_place: bool,
    fields: Vec<Field>,
    /// The files whose unsaved edits reopening the window would lose,
    /// while that is asked.
    asking: Option<Vec<String>>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl AppearanceTab {
    pub fn new(
        settings: Entity<SettingsModel>,
        unsaved: Unsaved,
        in_place: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = settings.read(cx).view().settings.appearance.clone();
        let mut subscriptions = vec![cx.observe_in(&settings, window, |this, _, window, cx| {
            this.sync(window, cx);
            cx.notify();
        })];
        let fields = FieldKind::ALL
            .into_iter()
            .enumerate()
            .map(|(at, kind)| {
                let placeholder = match kind {
                    FieldKind::UiFont => text::UI_FONT_PLACEHOLDER,
                    FieldKind::CodeFont => text::CODE_FONT_PLACEHOLDER,
                    FieldKind::UiSize | FieldKind::CodeSize => "",
                };
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(kind.saved(&appearance))
                });
                subscriptions.push(cx.subscribe_in(
                    &state,
                    window,
                    move |this, _, event: &InputEvent, window, cx| match event {
                        InputEvent::Change => {
                            if let Some(field) = this.fields.get_mut(at) {
                                field.edited = true;
                            }
                        }
                        // Leaving the field or Enter saves it.
                        InputEvent::Blur | InputEvent::PressEnter { .. } => {
                            this.save_field(at, window, cx)
                        }
                        InputEvent::Focus => {}
                    },
                ));
                Field {
                    kind,
                    state,
                    edited: false,
                }
            })
            .collect();
        Self {
            settings,
            unsaved,
            in_place,
            fields,
            asking: None,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Shows the saved values in the fields not being typed into.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = self.settings.read(cx).view().settings.appearance.clone();
        for field in &mut self.fields {
            if field.edited {
                continue;
            }
            let saved = field.kind.saved(&appearance);
            if field.state.read(cx).value() != saved {
                field
                    .state
                    .update(cx, |state, cx| state.set_value(saved, window, cx));
                // Setting it is no edit of the user's.
                field.edited = false;
            }
        }
    }

    fn save_field(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.fields.get_mut(at) else {
            return;
        };
        if !field.edited {
            return;
        }
        field.edited = false;
        let (kind, typed) = (field.kind, field.state.read(cx).value().to_string());
        let mut appearance = self.settings.read(cx).view().settings.appearance.clone();
        if !kind.apply(&typed, &mut appearance) {
            self.sync(window, cx);
            return;
        }
        let request = self.settings.update(cx, |settings, cx| {
            settings.set_appearance(move |saved| *saved = appearance, cx)
        });
        self.send(request, cx);
    }

    fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        let request = self
            .settings
            .update(cx, |settings, cx| settings.set_theme(theme, cx));
        self.send(request, cx);
    }

    fn set_code_theme(&mut self, mode: Mode, theme: CodeTheme, cx: &mut Context<Self>) {
        let request = self.settings.update(cx, |settings, cx| {
            settings.set_appearance(move |appearance| mode.set(appearance, theme), cx)
        });
        self.send(request, cx);
    }

    /// Turns the system title bar on or off; where the window reopens for
    /// it, unsaved conflict edits are asked about first.
    fn set_system_title_bar(&mut self, on: bool, cx: &mut Context<Self>) {
        let unsaved = if self.in_place {
            Vec::new()
        } else {
            (self.unsaved)(cx)
        };
        if !unsaved.is_empty() {
            self.asking = Some(unsaved);
            cx.notify();
            return;
        }
        self.save_title_bar(on, cx);
    }

    fn save_title_bar(&mut self, on: bool, cx: &mut Context<Self>) {
        self.asking = None;
        let request = self.settings.update(cx, |settings, cx| {
            settings.set_appearance(move |appearance| appearance.system_title_bar = on, cx)
        });
        self.send(request, cx);
    }

    /// Sends `request`; its refusal shows in the tab.
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

    fn theme_choices(&self, read_only: bool, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.read(cx).view().settings.theme;
        let buttons = THEMES.iter().enumerate().map(|(at, (theme, label))| {
            let theme = *theme;
            radio(("settings-theme", at), label, current == theme, read_only)
                .on_click(cx.listener(move |this, _, _, cx| this.set_theme(theme, cx)))
        });
        group("settings-theme", text::THEME)
            .child(radio_group("settings-themes", text::THEME).children(buttons))
            .into_any_element()
    }

    fn code_theme_choice(&self, mode: Mode, read_only: bool, cx: &mut Context<Self>) -> AnyElement {
        let appearance = self.settings.read(cx).view().settings.appearance.clone();
        let current = mode.of(&appearance);
        let buttons = CODE_THEMES.iter().enumerate().map(|(at, theme)| {
            let theme = *theme;
            radio(
                gpui_kit::ElementId::NamedInteger(mode.id().into(), at as u64),
                code_theme_label(theme),
                current == theme,
                read_only,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_code_theme(mode, theme, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(mode.label())
            .child(radio_group(mode.id(), mode.label()).children(buttons))
            .child(preview(mode, current, cx))
            .into_any_element()
    }

    fn font_fields(&self, read_only: bool) -> AnyElement {
        let field = |kind: FieldKind| self.fields.iter().find(|field| field.kind == kind);
        let row = |name: &'static str, font: FieldKind, size: FieldKind| {
            let size_label: SharedString = text::font_size(name).into();
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap_2()
                .children(field(font).map(|font| {
                    div().flex().flex_col().flex_1().gap_1().child(name).child(
                        Input::new(&font.state)
                            .id(font.kind.id())
                            .aria_label(name)
                            .disabled(read_only),
                    )
                }))
                .children(field(size).map(|size| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(size_label.clone())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .child(
                                    div().w(px(64.)).child(
                                        Input::new(&size.state)
                                            .id(size.kind.id())
                                            .aria_label(size_label.clone())
                                            .disabled(read_only),
                                    ),
                                )
                                .child(text::PIXELS),
                        )
                }))
        };
        group("settings-fonts", text::FONTS)
            .child(row(text::UI_FONT, FieldKind::UiFont, FieldKind::UiSize))
            .child(row(
                text::CODE_FONT,
                FieldKind::CodeFont,
                FieldKind::CodeSize,
            ))
            .into_any_element()
    }

    fn window_section(&self, read_only: bool, cx: &mut Context<Self>) -> AnyElement {
        let on = self
            .settings
            .read(cx)
            .view()
            .settings
            .appearance
            .system_title_bar;
        let mut section = group("settings-window", text::WINDOW_SECTION).child(
            Checkbox::new("settings-system-title-bar")
                .label(text::SYSTEM_TITLE_BAR)
                .checked(on)
                .disabled(read_only || self.asking.is_some())
                .on_click(cx.listener(|this, value: &bool, _, cx| {
                    this.set_system_title_bar(*value, cx);
                })),
        );
        if let Some(files) = &self.asking {
            let question: SharedString = text::REOPEN_LOST.into();
            section = section.child(
                div()
                    .id("settings-reopen-question")
                    .test_support()
                    .role(Role::Group)
                    .aria_label(question.clone())
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(question)
                    .child(
                        div()
                            .id("settings-reopen-files")
                            .test_support()
                            .role(Role::List)
                            .pl_4()
                            .children(files.iter().enumerate().map(|(at, file)| {
                                let file: SharedString = file.clone().into();
                                div()
                                    .id(("settings-reopen-file", at))
                                    .test_support()
                                    .role(Role::ListItem)
                                    .aria_label(file.clone())
                                    .child(file)
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                Button::new("settings-reopen-cancel")
                                    .label(common::CANCEL)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.asking = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("settings-reopen-confirm")
                                    .danger()
                                    .label(text::REOPEN)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.save_title_bar(!on, cx);
                                    })),
                            ),
                    ),
            );
        }
        section.into_any_element()
    }
}

/// A group of settings under its heading.
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

fn radio_group(
    id: &'static str,
    label: &'static str,
) -> impl ParentElement + IntoElement + Styled + use<> {
    div()
        .id(SharedString::from(format!("{id}-choices")))
        .test_support()
        .role(Role::RadioGroup)
        .aria_label(label)
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_1()
}

/// A choice of a radio group, as the provider form's kinds are.
fn radio(
    id: impl Into<gpui_kit::ElementId>,
    label: &'static str,
    chosen: bool,
    read_only: bool,
) -> Button {
    Button::new(id)
        .label(label)
        .selected(chosen)
        .role(Role::RadioButton)
        .toggled(chosen)
        .disabled(read_only)
}

/// A few lines of a diff in `theme`, whatever the app theme is, as rc.1's
/// `CodeThemePreview.tsx` draws them (UI-24).
fn preview(mode: Mode, theme: CodeTheme, cx: &App) -> AnyElement {
    let colors = &code_theme(theme).colors;
    let line = |runs: &[(&str, Syntax)]| {
        let mut text = String::new();
        let mut highlights = Vec::new();
        for (part, syntax) in runs {
            let start = text.len();
            text.push_str(part);
            highlights.push((
                start..text.len(),
                HighlightStyle {
                    color: Some(colors.syntax(*syntax).into()),
                    ..HighlightStyle::default()
                },
            ));
        }
        StyledText::new(text).with_highlights(highlights)
    };
    use Syntax::{Comment, Function, Keyword, Number, Plain, String as Str, Type};
    let code_font = crate::theme::AppTheme::global(cx).code_font().clone();
    // The id names the theme, so a test can tell which one is drawn.
    let at = CODE_THEMES
        .iter()
        .position(|known| *known == theme)
        .unwrap_or_default();
    let id = match mode {
        Mode::Light => "settings-preview-light",
        Mode::Dark => "settings-preview-dark",
    };
    div()
        .id(gpui_kit::ElementId::NamedInteger(id.into(), at as u64))
        .test_support()
        .font_family(code_font)
        .text_sm()
        .bg(gpui_kit::Hsla::from(colors.background))
        .text_color(gpui_kit::Hsla::from(colors.text))
        .border_1()
        .border_color(cx.tokens().border)
        .rounded(px(4.))
        .child(
            div()
                .px_2()
                .bg(gpui_kit::Hsla::from(colors.hunk_background))
                .text_color(gpui_kit::Hsla::from(colors.hunk_text))
                .child("@@ -1,4 +1,4 @@"),
        )
        .child(div().px_2().child(line(&[
            ("function", Keyword),
            (" ", Plain),
            ("greet", Function),
            ("(name: ", Plain),
            ("string", Type),
            (") {", Plain),
        ])))
        .child(
            div()
                .px_2()
                .bg(gpui_kit::Hsla::from(colors.removed))
                .child(line(&[
                    ("  ", Plain),
                    ("return", Keyword),
                    (" ", Plain),
                    ("\"Hello \"", Str),
                    (" + name; ", Plain),
                    ("// old", Comment),
                ])),
        )
        .child(
            div()
                .px_2()
                .bg(gpui_kit::Hsla::from(colors.added))
                .child(line(&[
                    ("  ", Plain),
                    ("return", Keyword),
                    (" ", Plain),
                    ("\"Hello, \"", Str),
                    (" + name + ", Plain),
                    ("1", Number),
                    (";", Plain),
                ])),
        )
        .child(div().px_2().child("}"))
        .into_any_element()
}

impl Render for AppearanceTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let read_only = self.settings.read(cx).read_only();
        let danger = cx.tokens().danger;
        div()
            .id("settings-appearance")
            .test_support()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.theme_choices(read_only, cx))
            .child(
                group("settings-code-themes", text::CODE_THEMES)
                    .child(self.code_theme_choice(Mode::Light, read_only, cx))
                    .child(self.code_theme_choice(Mode::Dark, read_only, cx)),
            )
            .child(self.font_fields(read_only))
            .child(self.window_section(read_only, cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .id("settings-appearance-error")
                        .test_support()
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .text_color(danger)
                        .child(error),
                )
            })
    }
}
