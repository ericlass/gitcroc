//! WCAG 2 contrast (README "Accessibility target"), and the system accent
//! made to keep it (UI-23).

use super::tokens::{Color, Tokens};

/// WCAG 2 relative luminance of an opaque colour.
pub fn luminance(color: Color) -> f64 {
    let [r, g, b] = color.channels().map(|channel| {
        let value = f64::from(channel) / 255.;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG 2 contrast ratio, 1 to 21.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Text against what it is drawn on (WCAG 1.4.3).
pub const TEXT: f64 = 4.5;
/// Focus rings, input borders and selection marks (WCAG 1.4.11).
pub const NON_TEXT: f64 = 3.;

/// Whether `accent` keeps every ratio the accent takes part in, in
/// `tokens`: as text on each background, under `accent_text`, and as the
/// focus ring and selection mark.
fn accent_passes(accent: Color, tokens: &Tokens) -> bool {
    tokens
        .backgrounds()
        .iter()
        .all(|background| contrast(accent, *background) >= TEXT)
        && contrast(tokens.accent_text, accent) >= TEXT
}

/// The accent to use for `system`: as it is when it keeps contrast in
/// `tokens`, else the same hue at the nearest lightness that does, else the
/// tokens' own accent.
pub fn accent_for(system: Color, tokens: &Tokens) -> Color {
    if accent_passes(system, tokens) {
        return system;
    }
    let start = system.lightness();
    // Steps of 0.5% lightness, nearest first, either way.
    (1..=200)
        .flat_map(|step| {
            let delta = step as f32 * 0.005;
            [start - delta, start + delta]
        })
        .filter(|lightness| (0. ..=1.).contains(lightness))
        .map(|lightness| system.with_lightness(lightness))
        .find(|accent| accent_passes(*accent, tokens))
        .unwrap_or(tokens.accent)
}
