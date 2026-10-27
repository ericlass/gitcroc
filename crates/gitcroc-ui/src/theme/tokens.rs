//! The only place app colors are defined (UI-23); code theme colors are in
//! `code_themes.rs` (UI-24). Views read them through `cx.tokens()`; the
//! colour scan (`tests/integration/colors.rs`) fails on a colour anywhere
//! else. The values are 1.0.0-rc.1's `ui/src/theme/tokens.css`.

use gpui_kit::{Fill, Hsla, Pixels, Rgba, px};

/// An opaque colour, `0xrrggbb`, so the contrast the tests compute is the
/// one on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    /// Red, green and blue, 0 to 255.
    pub fn channels(self) -> [u8; 3] {
        let [_, r, g, b] = self.0.to_be_bytes();
        [r, g, b]
    }

    pub fn from_channels([r, g, b]: [u8; 3]) -> Self {
        Self(u32::from_be_bytes([0, r, g, b]))
    }

    /// `#rrggbb`, as gpui-component's theme files write colours.
    pub fn hex(self) -> String {
        format!("#{:06x}", self.0)
    }

    /// Its HSL lightness, 0 to 1.
    pub fn lightness(self) -> f32 {
        Hsla::from(self).l
    }

    /// The colour with the same hue and saturation at `lightness`, 0 to 1.
    pub fn with_lightness(self, lightness: f32) -> Self {
        let mut color = Hsla::from(self);
        color.l = lightness.clamp(0., 1.);
        let rgba = color.to_rgb();
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
        Self::from_channels([channel(rgba.r), channel(rgba.g), channel(rgba.b)])
    }

    /// `#rrggbbaa` at `opacity`, 0 to 1.
    pub fn hex_alpha(self, opacity: f32) -> String {
        let alpha = (opacity.clamp(0., 1.) * 255.).round() as u8;
        format!("{}{alpha:02x}", self.hex())
    }

    /// With `opacity`, 0 to 1.
    pub fn alpha(self, opacity: f32) -> Hsla {
        let mut color = Hsla::from(self);
        color.a = opacity;
        color
    }
}

impl From<Color> for Hsla {
    fn from(color: Color) -> Self {
        let [r, g, b] = color.channels();
        Rgba {
            r: f32::from(r) / 255.,
            g: f32::from(g) / 255.,
            b: f32::from(b) / 255.,
            a: 1.,
        }
        .into()
    }
}

impl From<Color> for Fill {
    fn from(color: Color) -> Self {
        Hsla::from(color).into()
    }
}

/// The colours of one app theme, light or dark (UI-23).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// Backgrounds: window, bars and panes, raised (hover, pressed, dialogs).
    pub bg_1: Color,
    pub bg_2: Color,
    pub bg_3: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    /// Pane and row borders.
    pub border: Color,
    /// An input's border, which must stand out from its background (WCAG
    /// 1.4.11).
    pub border_input: Color,
    /// Text colours keep 4.5:1 on each background and the selection.
    pub accent: Color,
    pub accent_text: Color,
    pub danger: Color,
    pub warning: Color,
    pub warning_bg: Color,
    pub success: Color,
    pub focus_ring: Color,
    /// A selected row: its background and the mark at its edge, which alone
    /// keeps 3:1.
    pub selection_bg: Color,
    pub selection_mark: Color,
    /// Behind a dialog, at [`Self::backdrop_opacity`].
    pub backdrop: Color,
    pub backdrop_opacity: f32,
    /// History graph lines (UI-9), by the core's colour index modulo 8.
    pub graph: [Color; 8],
    /// Job outcomes (GRP-7, UI-21).
    pub outcome_succeeded: Color,
    pub outcome_nothing_to_do: Color,
    pub outcome_skipped: Color,
    pub outcome_failed: Color,
    pub outcome_conflict: Color,
}

impl Tokens {
    pub const LIGHT: Self = Self {
        bg_1: Color(0xffffff),
        bg_2: Color(0xf5f6f8),
        bg_3: Color(0xe8eaee),
        text_primary: Color(0x1f2328),
        text_secondary: Color(0x59636e),
        text_disabled: Color(0x9ba3ad),
        border: Color(0xd1d9e0),
        border_input: Color(0x787f88),
        accent: Color(0x0964cf),
        accent_text: Color(0xffffff),
        danger: Color(0xc9212d),
        warning: Color(0x8e5f00),
        warning_bg: Color(0xfff8c5),
        success: Color(0x187734),
        focus_ring: Color(0x0964cf),
        selection_bg: Color(0xe8eaee),
        selection_mark: Color(0x0964cf),
        backdrop: Color(0x1f2328),
        backdrop_opacity: 0.4,
        graph: [
            Color(0x0969da),
            Color(0x1a7f37),
            Color(0x8250df),
            Color(0xbc4c00),
            Color(0xbf3989),
            Color(0x1b7c83),
            Color(0xcf222e),
            Color(0x9a6700),
        ],
        outcome_succeeded: Color(0x187734),
        outcome_nothing_to_do: Color(0x59636e),
        outcome_skipped: Color(0x8e5f00),
        outcome_failed: Color(0xc9212d),
        outcome_conflict: Color(0xb14700),
    };

    pub const DARK: Self = Self {
        bg_1: Color(0x0d1117),
        bg_2: Color(0x151b23),
        bg_3: Color(0x212830),
        text_primary: Color(0xe6edf3),
        text_secondary: Color(0x9198a1),
        text_disabled: Color(0x5c646d),
        border: Color(0x3d444d),
        border_input: Color(0x6e7681),
        accent: Color(0x4493f8),
        accent_text: Color(0x0d1117),
        danger: Color(0xf85850),
        warning: Color(0xd29922),
        warning_bg: Color(0x2e2410),
        success: Color(0x3fb950),
        focus_ring: Color(0x4493f8),
        selection_bg: Color(0x212830),
        selection_mark: Color(0x4493f8),
        backdrop: Color(0x010409),
        backdrop_opacity: 0.6,
        graph: [
            Color(0x4493f8),
            Color(0x3fb950),
            Color(0xab7df8),
            Color(0xf0883e),
            Color(0xf778ba),
            Color(0x39c5cf),
            Color(0xf85149),
            Color(0xd29922),
        ],
        outcome_succeeded: Color(0x3fb950),
        outcome_nothing_to_do: Color(0x9198a1),
        outcome_skipped: Color(0xd29922),
        outcome_failed: Color(0xf85850),
        outcome_conflict: Color(0xf0883e),
    };

    /// The backgrounds text is drawn on.
    pub fn backgrounds(&self) -> [Color; 4] {
        [self.bg_1, self.bg_2, self.bg_3, self.selection_bg]
    }

    /// With `accent` as the accent, the focus ring and the selection mark.
    pub fn with_accent(self, accent: Color) -> Self {
        Self {
            accent,
            focus_ring: accent,
            selection_mark: accent,
            ..self
        }
    }
}

pub const RADIUS: Pixels = px(4.);
pub const SPLITTER_WIDTH: Pixels = px(5.);
