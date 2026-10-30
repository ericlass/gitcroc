//! The card beside a hunk (M6-02): its base on request, and a supplied AI
//! proposal with its explanation and confidence, Accept, Reject and Edit
//! (AI-11), as rc.1's `BaseBlock` and `ProposalBlock`. Accept goes through
//! the same transaction as a take, into the buffer only (AI-12). What the
//! user did with each proposal stays with the editor, which is kept per
//! file; supplying no proposal, as with AI features off, drops it (AI-16).
use gitcroc_core::{FileProposal, ProposalConfidence, ProposalStatus, ProposedHunk};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Editor, EditorState};
use gpui_component::{Disableable, Sizable, h_flex, v_flex};
use gpui_kit::{prelude::*, *};

use super::{ConflictEditor, Look, buffer};
use crate::strings::{common, conflict as text};
use crate::theme::AppTheme;
use crate::theme::tokens::Tokens;

actions!(
    conflict,
    [
        /// Ctrl+Enter in a proposal's editor: accept the edited text.
        AcceptEdited,
        /// Escape in a proposal's editor: close it.
        CancelEdit
    ]
);

/// The key context of a proposal's editor.
pub const EDITING: &str = "GitcrocProposalEdit";

/// What the user did with one hunk's proposal.
#[derive(Default)]
pub(super) struct Review {
    rejected: bool,
    editing: Option<Entity<EditorState>>,
}

fn id(name: String) -> ElementId {
    ElementId::Name(name.into())
}

impl ConflictEditor {
    /// The core's proposal for this file, or `None`: no proposal, or AI
    /// features off. A different proposal starts its review afresh.
    pub fn set_proposal(&mut self, proposal: Option<FileProposal>, cx: &mut Context<Self>) {
        let hunks = proposal.map(|proposal| proposal.hunks).unwrap_or_default();
        if hunks != self.proposal {
            self.proposal = hunks;
            self.reviews.clear();
        }
        cx.notify();
    }
    /// Hunk `hunk`'s proposal while shown: proposed hunks count from 1.
    fn proposed(&self, hunk: usize) -> Option<&ProposedHunk> {
        let rejected = self.reviews.get(&hunk).is_some_and(|r| r.rejected);
        self.proposal
            .iter()
            .find(|p| p.id as usize == hunk + 1)
            .filter(|_| !rejected)
    }
    fn untouched(&self, hunk: usize, cx: &App) -> bool {
        self.ranges(cx)
            .iter()
            .any(|(at, _, untouched)| *at == hunk && *untouched)
    }
    /// Accept (AI-12): the proposed text, or the edited one, replaces the
    /// untouched hunk, markers included. Refused once the hunk changed.
    pub fn accept_proposal(
        &mut self,
        hunk: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let edited = self.edited_text(hunk, cx);
        let Some(proposed) = self.proposed(hunk) else {
            return false;
        };
        let Some(proposed_text) = proposed.resolution.clone() else {
            return false;
        };
        let accepted = self.accept(hunk, &edited.unwrap_or(proposed_text), window, cx);
        if accepted {
            self.reviews.entry(hunk).or_default().editing = None;
        }
        accepted
    }
    pub fn reject_proposal(&mut self, hunk: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.proposed(hunk).is_some() {
            *self.reviews.entry(hunk).or_default() = Review {
                rejected: true,
                editing: None,
            };
            self.editor.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
        }
    }
    /// Opens the proposed text in an editor of its own, focused.
    pub fn edit_proposal(
        &mut self,
        hunk: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(resolution) = self.proposed(hunk).and_then(|p| p.resolution.clone()) else {
            return false;
        };
        if !self.untouched(hunk, cx) {
            return false;
        }
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx).line_number(false);
            state.set_value(resolution, window, cx);
            state
        });
        editor.update(cx, |state, cx| state.focus(window, cx));
        self.reviews.entry(hunk).or_default().editing = Some(editor);
        cx.notify();
        true
    }
    fn cancel_edit(&mut self, hunk: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.reviews.entry(hunk).or_default().editing = None;
        self.editor.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }
    /// The text in hunk `hunk`'s proposal editor, while open.
    pub fn edited_text(&self, hunk: usize, cx: &App) -> Option<String> {
        self.proposal_editor(hunk)
            .map(|editor| editor.read(cx).value().to_string())
    }
    pub fn proposal_editor(&self, hunk: usize) -> Option<Entity<EditorState>> {
        self.reviews.get(&hunk)?.editing.clone()
    }
    /// Accept all high-confidence (AI-11): this file's untouched hunks
    /// proposed with high confidence, in one undo step. Everything between
    /// them is written back as it was, and its hunks keep their anchors.
    pub fn accept_all_high(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let live = self.ranges(cx);
        let chosen: Vec<_> = live
            .iter()
            .filter(|(_, _, untouched)| *untouched)
            .filter_map(|(hunk, range, _)| {
                let proposed = self.proposed(*hunk)?;
                let resolution = proposed.resolution.as_ref()?;
                (proposed.status == ProposalStatus::Resolved
                    && proposed.confidence == ProposalConfidence::High)
                    .then(|| (*hunk, range.clone(), buffer::normalize(resolution)))
            })
            .collect();
        let (Some(first), Some(last)) = (chosen.first(), chosen.last()) else {
            return false;
        };
        let span = first.1.start..last.1.end;
        let old = self.value(cx);
        let mut replaced = String::new();
        let mut at = span.start;
        // Where the hunks between them land, by how much the text before
        // each grew or shrank.
        let mut moved = Vec::new();
        let mut shift = 0isize;
        for (hunk, range, _) in &live {
            if range.start < span.start || range.end > span.end {
                continue;
            }
            if let Some((_, _, text)) = chosen.iter().find(|(at, _, _)| at == hunk) {
                replaced.push_str(&old[at..range.start]);
                replaced.push_str(text);
                at = range.end;
                shift += text.len() as isize - range.len() as isize;
            } else {
                let start = range.start.saturating_add_signed(shift);
                moved.push((*hunk, start..start + range.len()));
            }
        }
        replaced.push_str(&old[at..span.end]);
        for (hunk, _, _) in &chosen {
            self.hunks[*hunk].taken = true;
            self.pending_takes.push(*hunk);
            self.reviews.entry(*hunk).or_default().editing = None;
        }
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(span.clone(), cx);
            state.replace(replaced, window, cx);
            state.set_selected_range(span.start..span.start, cx);
            state.focus(window, cx);
        });
        for (hunk, range) in moved {
            let tracked = &self.hunks[hunk];
            tracked
                .anchor
                .set(vec![super::decoration(range.clone())], cx);
            tracked
                .before
                .set(vec![super::decoration(0..range.start)], cx);
        }
        true
    }

    /// The card of hunk `hunk`, the `number`th left, if it shows anything.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn card(
        &self,
        hunk: usize,
        number: usize,
        untouched: bool,
        max_height: Option<Pixels>,
        line_height: Pixels,
        look: &Look,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let which = text::which(number);
        let size = AppTheme::global(cx).code_font_size();
        let base = (untouched && self.show_base.contains(&hunk))
            .then(|| self.base(hunk, &which, look, cx).into_any_element());
        let proposal = self.proposed(hunk).cloned().map(|proposed| {
            self.proposal(
                hunk,
                number,
                &which,
                &proposed,
                untouched,
                line_height,
                look,
                tokens,
                cx,
            )
            .into_any_element()
        });
        if base.is_none() && proposal.is_none() {
            return None;
        }
        Some(
            v_flex()
                .key_context(super::overlay::CONTROLS)
                .on_action(
                    cx.listener(move |this, _: &super::ReturnToText, window, cx| {
                        this.return_to_text(hunk, window, cx)
                    }),
                )
                .gap_1()
                .p_1()
                .rounded(px(4.))
                .border_1()
                .border_color(tokens.border)
                .bg(tokens.bg_1)
                .shadow_md()
                .text_color(tokens.text_primary)
                .text_size(size)
                .when_some(max_height, |card, height| card.max_h(height))
                .children(base)
                .children(proposal),
        )
    }

    fn base(&self, hunk: usize, which: &str, look: &Look, cx: &App) -> impl IntoElement {
        let base = &self.hunks[hunk].hunk.base;
        let shown: SharedString = if base.is_empty() {
            text::NOTHING.into()
        } else {
            base.trim_end_matches('\n').to_owned().into()
        };
        v_flex()
            .id(id(format!("conflict-base-{hunk}")))
            .test_support()
            .role(Role::Group)
            .aria_label(text::base_of(which))
            .bg(look.rows)
            .border_l_4()
            .border_color(look.marker)
            .px_2()
            .py_1()
            .text_color(look.text)
            .child(div().font_weight(FontWeight::SEMIBOLD).child(text::BASE))
            .child(code(format!("conflict-base-text-{hunk}"), shown, cx))
    }

    #[allow(clippy::too_many_arguments)]
    fn proposal(
        &self,
        hunk: usize,
        number: usize,
        which: &str,
        proposed: &ProposedHunk,
        untouched: bool,
        line_height: Pixels,
        look: &Look,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (word, _) = text::confidence(proposed.confidence);
        let level = match proposed.confidence {
            ProposalConfidence::High => 0,
            ProposalConfidence::Medium => 1,
            ProposalConfidence::Low => 2,
        };
        let note = |name: &str, words: &'static str| {
            div()
                .id(id(format!("conflict-{name}-{hunk}")))
                .test_support()
                .aria_label(words)
                .italic()
                .child(words)
        };
        // After the hunk's own controls in its tab slot (see `overlay.rs`).
        let slot = (number as isize) * 10;
        let button = |part: &str, name: &'static str, label: String, offset: isize| {
            Button::new(id(format!("conflict-{hunk}-{part}")))
                .xsmall()
                .compact()
                .outline()
                .label(name)
                .accessibility_label(label)
                .tab_index(slot + offset)
        };
        let editing = self.proposal_editor(hunk);
        let body = match (&editing, &proposed.resolution) {
            (Some(editor), _) => Some(
                div()
                    .key_context(EDITING)
                    .on_action(cx.listener(move |this, _: &AcceptEdited, window, cx| {
                        this.accept_proposal(hunk, window, cx);
                    }))
                    .on_action(cx.listener(move |this, _: &CancelEdit, window, cx| {
                        this.cancel_edit(hunk, window, cx)
                    }))
                    .child(
                        Editor::new(editor)
                            .h(line_height * 6.)
                            .aria_label(text::edited_proposal_for(which)),
                    )
                    .into_any_element(),
            ),
            (None, Some(resolution)) => Some(
                code(
                    format!("conflict-proposal-text-{hunk}"),
                    resolution.trim_end_matches('\n').to_owned().into(),
                    cx,
                )
                .into_any_element(),
            ),
            (None, None) => None,
        };
        let accept = button(
            "accept",
            text::ACCEPT,
            if editing.is_some() {
                text::accept_edited_for(which)
            } else {
                text::accept_for(which)
            },
            4,
        )
        .primary()
        .disabled(!untouched)
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.accept_proposal(hunk, window, cx);
        }));
        let reject =
            button("reject", text::REJECT, text::reject_for(which), 5).on_click(cx.listener(
                move |this, _: &ClickEvent, window, cx| this.reject_proposal(hunk, window, cx),
            ));
        let buttons = h_flex()
            .flex_wrap()
            .gap_1()
            .p_1()
            .bg(tokens.bg_1)
            .text_color(tokens.text_primary)
            .map(|row| match (&editing, &proposed.resolution) {
                (Some(_), _) => row.child(accept).child(
                    button("cancel", common::CANCEL, text::cancel_edit_for(which), 5).on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.cancel_edit(hunk, window, cx)
                        }),
                    ),
                ),
                (None, Some(_)) => row.child(accept).child(reject).child(
                    button("edit", text::EDIT, text::edit_for(which), 6)
                        .disabled(!untouched)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.edit_proposal(hunk, window, cx);
                        })),
                ),
                (None, None) => row.child(reject),
            });
        v_flex()
            .id(id(format!("conflict-proposal-{hunk}")))
            .test_support()
            .role(Role::Group)
            .aria_label(text::proposal_for(which))
            .gap_1()
            .bg(look.proposal)
            .border_l_4()
            .border_color(look.proposal_edge)
            .px_2()
            .py_1()
            .text_color(look.text)
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text::AI_PROPOSAL),
                    )
                    .child(
                        div()
                            .id(id(format!("conflict-confidence-{hunk}")))
                            .test_support()
                            .aria_label(word)
                            .px_1()
                            .rounded(px(3.))
                            .bg(look.confidence[level])
                            .child(text::confidence_label(proposed.confidence)),
                    )
                    .when(proposed.status == ProposalStatus::Unresolved, |head| {
                        head.child(note("unresolved", text::NOT_RESOLVED))
                    })
                    .when(!untouched, |head| {
                        head.child(note("changed", text::HUNK_CHANGED))
                    }),
            )
            .children(body)
            // The model's words, as text (SEC-3).
            .child(
                div()
                    .id(id(format!("conflict-explanation-{hunk}")))
                    .test_support()
                    .role(Role::Paragraph)
                    .aria_value(SharedString::from(proposed.explanation.clone()))
                    .child(proposed.explanation.clone()),
            )
            .child(buttons)
    }
}

/// Repo text or proposed text, in the code font, as text (SEC-3).
fn code(name: String, shown: SharedString, cx: &App) -> impl IntoElement {
    let theme = AppTheme::global(cx);
    div()
        .id(id(name))
        .test_support()
        .role(Role::Paragraph)
        .aria_value(shown.clone())
        .font_family(theme.code_font().clone())
        .child(shown)
}
