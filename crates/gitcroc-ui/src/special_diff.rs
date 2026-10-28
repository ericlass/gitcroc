//! The diffs that are not drawn as text (UI-18), a port of rc.1's
//! `changes/SpecialDiff.tsx`: a binary file with its sizes, an image before
//! and after, a diff over the line limit until the user loads it, an
//! untracked folder and a submodule's commits.
//!
//! An image is drawn from the bytes the core read, decoded off the UI
//! thread (ARCH-7); no file or URL is made for it (SEC-2).

use std::sync::Arc;

use gitcroc_core::{FileDiff, ImageFormat};
use gpui_component::button::{Button, ButtonVariants};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, Image, ImageFormat as Format, InteractiveElement, IntoElement,
    ObjectFit, ParentElement, RenderImage, Role, SharedString, StatefulInteractiveElement, Styled,
    StyledImage, SvgRenderer, TestSupportExt, Window, div, img,
};

use crate::strings::changes as text;
use crate::theme::tokens::Tokens;

/// One side of an image diff.
#[derive(Clone)]
pub struct Picture {
    /// The format's name, as "PNG".
    pub format: &'static str,
    pub size: u64,
    /// The decoded image; `None` when it could not be decoded.
    pub image: Option<Arc<RenderImage>>,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("format", &self.format)
            .field("size", &self.size)
            .field("decoded", &self.image.is_some())
            .finish()
    }
}

/// A diff that is not text.
#[derive(Debug, Clone)]
pub enum Special {
    /// Sizes in bytes; `None` for a side that does not exist.
    Binary {
        old: Option<u64>,
        new: Option<u64>,
    },
    Image {
        old: Option<Picture>,
        new: Option<Picture>,
    },
    TooLarge {
        added: u32,
        removed: u32,
    },
    Folder,
    Submodule {
        old: Option<String>,
        new: Option<String>,
    },
}

/// A size in bytes for people: "68 B", "1.2 KB", "3.4 MB".
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return text::bytes(bytes);
    }
    let mut value = bytes as f64 / 1024.;
    let mut unit = 0;
    while value >= 1024. && unit < text::SIZE_UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    text::size(value, text::SIZE_UNITS[unit])
}

fn size_text(size: Option<u64>) -> String {
    size.map_or_else(|| text::NONE.to_owned(), format_size)
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => text::PNG,
        ImageFormat::Jpeg => text::JPEG,
        ImageFormat::Gif => text::GIF,
        ImageFormat::Webp => text::WEBP,
        ImageFormat::Bmp => text::BMP,
    }
}

fn picture(image: gitcroc_core::Image, svg: &SvgRenderer) -> Picture {
    let format = match image.format {
        ImageFormat::Png => Format::Png,
        ImageFormat::Jpeg => Format::Jpeg,
        ImageFormat::Gif => Format::Gif,
        ImageFormat::Webp => Format::Webp,
        ImageFormat::Bmp => Format::Bmp,
    };
    Picture {
        format: format_name(image.format),
        size: image.size,
        image: Image::from_bytes(format, image.bytes)
            .to_image_data(svg.clone())
            .ok(),
    }
}

impl Special {
    /// The special diff of `diff`, decoding its images; `None` for text.
    pub fn build(diff: FileDiff, svg: &SvgRenderer) -> Option<Self> {
        Some(match diff {
            FileDiff::Text(_) => return None,
            FileDiff::Binary { old_size, new_size } => Self::Binary {
                old: old_size,
                new: new_size,
            },
            FileDiff::Image { old, new } => Self::Image {
                old: old.map(|image| picture(image, svg)),
                new: new.map(|image| picture(image, svg)),
            },
            FileDiff::TooLarge { added, removed } => Self::TooLarge { added, removed },
            FileDiff::Directory => Self::Folder,
            FileDiff::Submodule { old, new } => Self::Submodule { old, new },
        })
    }

    /// The decoded images, to release from the GPU when the diff goes.
    pub fn images(&self) -> impl Iterator<Item = Arc<RenderImage>> + '_ {
        let (old, new) = match self {
            Self::Image { old, new } => (old.as_ref(), new.as_ref()),
            _ => (None, None),
        };
        [old, new]
            .into_iter()
            .flatten()
            .filter_map(|picture| picture.image.clone())
    }
}

fn paragraph(id: &'static str, label: String) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .role(Role::Paragraph)
        .aria_label(label.clone())
        .child(label)
        .into_any_element()
}

/// One side of an image diff: its caption, then the image, scaled down to
/// the pane's width and never up.
fn image_side(
    id: &'static str,
    side: &'static str,
    picture: Option<&Picture>,
    tokens: &Tokens,
) -> AnyElement {
    let what = picture.map_or_else(
        || text::NONE.to_owned(),
        |picture| text::image_kind(picture.format, &format_size(picture.size)),
    );
    let caption = text::image_caption(side, &what);
    div()
        .id(id)
        .test_support()
        .role(Role::Figure)
        .aria_label(caption.clone())
        .flex()
        .flex_col()
        .items_start()
        .gap_1()
        .flex_1()
        .min_w(gpui_kit::px(200.))
        .child(div().text_color(tokens.text_secondary).child(caption))
        .when_some(
            picture.and_then(|picture| picture.image.clone()),
            |figure, image| {
                figure.child(
                    div()
                        .id(SharedString::from(format!("{id}-image")))
                        .test_support()
                        .role(Role::Image)
                        .aria_label(text::image(side))
                        .flex()
                        .max_w_full()
                        .border_1()
                        .border_color(tokens.border)
                        .bg(tokens.bg_2)
                        .child(img(image).max_w_full().object_fit(ObjectFit::ScaleDown)),
                )
            },
        )
        .into_any_element()
}

/// Draws `special`; `on_load` reads a large diff anyway.
pub fn render(
    special: &Special,
    tokens: &Tokens,
    on_load: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let body = div()
        .id("diff-special")
        .test_support()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .text_color(tokens.text_primary);
    match special {
        Special::Binary { old, new } => body.child(paragraph(
            "diff-binary",
            text::binary(&size_text(*old), &size_text(*new)),
        )),
        Special::Image { old, new } => body.child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_4()
                .child(image_side(
                    "diff-image-old",
                    text::BEFORE,
                    old.as_ref(),
                    tokens,
                ))
                .child(image_side(
                    "diff-image-new",
                    text::AFTER,
                    new.as_ref(),
                    tokens,
                )),
        ),
        Special::TooLarge { added, removed } => body
            .child(paragraph("diff-large", text::large(*added, *removed)))
            .child(
                div().child(
                    Button::new("diff-load")
                        .primary()
                        .label(text::LOAD_DIFF)
                        .on_click(on_load),
                ),
            ),
        Special::Folder => body.child(paragraph("diff-folder", text::UNTRACKED_FOLDER.to_owned())),
        Special::Submodule { old, new } => {
            let side = |id: &'static str, label: &'static str, commit: &Option<String>| {
                let commit = commit.as_deref().unwrap_or(text::NONE);
                div()
                    .id(id)
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_label(text::submodule(label, commit))
                    .flex()
                    .flex_col()
                    .child(div().text_color(tokens.text_secondary).child(label))
                    .child(SharedString::from(commit.to_owned()))
            };
            body.child(side("diff-submodule-old", text::SUBMODULE_BEFORE, old))
                .child(side("diff-submodule-new", text::SUBMODULE_AFTER, new))
        }
    }
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_18_sizes_read_as_b_kb_mb_and_gb() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(10 * 1024 * 1024 + 1), "10.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(format_size(5000 * 1024 * 1024 * 1024), "5000.0 GB");
    }

    #[test]
    fn ui_18_large_counts_are_grouped() {
        assert_eq!(
            text::large(20_001, 0),
            "This diff is large: 20,001 lines added and 0 removed. Showing it may take a while."
        );
        assert_eq!(
            text::large(1_234_567, 999),
            "This diff is large: 1,234,567 lines added and 999 removed. Showing it may take a while."
        );
    }
}
