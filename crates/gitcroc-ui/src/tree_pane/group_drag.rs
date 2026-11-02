//! The tree's group changes (UI-7): what a row menu's group entries act
//! on, and dragging repos and groups, a port of rc.1's drag handling in
//! `tree/TreePane.tsx`. The rules are `crate::groups`'; the core applies
//! the change and the tree follows its `WorkspaceChanged` (ARCH-6).

use gitcroc_core::{RepoId, TargetSet, TreeItem};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, DragMoveEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window, div, px,
};

use super::TreePane;
use crate::groups::{self, Dragged, DropPlace, GroupInfo, GroupOp};
use crate::strings::groups as text;
use crate::theme::ActiveTokens;

/// A row being dragged: its row, what it drags, and its label for the
/// preview.
#[derive(Debug, Clone)]
pub struct DraggedRow {
    row: String,
    label: SharedString,
    what: Dragged,
}

/// What follows the pointer while a row is dragged.
struct DraggedRowView(SharedString);

impl Render for DraggedRowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens();
        div()
            .px_2()
            .py_0p5()
            .text_sm()
            .bg(tokens.bg_3)
            .text_color(tokens.text_primary)
            .border_1()
            .border_color(tokens.border)
            .rounded(px(4.))
            .child(self.0.clone())
    }
}

/// The drag in progress: the dragged row, and where a drop would land.
#[derive(Debug, Clone, Default)]
pub(super) struct Drag {
    row: Option<String>,
    target: Option<(String, DropPlace)>,
}

impl TreePane {
    /// The workspace's groups, and the repos a group entry of `item`'s menu
    /// acts on: the multi-selection a repo row is in, else its repo alone
    /// (GRP-5).
    pub(super) fn group_context(&self, item: &TreeItem) -> (Vec<GroupInfo>, Vec<RepoId>) {
        let groups = groups::groups_of(self.tree.roots());
        let chosen = match (item, self.tree.target()) {
            (TreeItem::Repo { repo_id, .. }, Some(TargetSet::Repos { repo_ids }))
                if repo_ids.contains(repo_id) =>
            {
                repo_ids
            }
            (TreeItem::Repo { repo_id, .. }, _) => vec![repo_id.clone()],
            _ => Vec::new(),
        };
        (groups, chosen)
    }

    /// Sends a change of the groups; a refusal shows below the tree.
    pub fn send_group_op(&mut self, op: GroupOp, cx: &mut Context<Self>) {
        self.operation_error = None;
        cx.notify();
        let workspace = self.workspace_id.clone();
        let call = self
            .services
            .with_app(move |app| groups::send(app, workspace, op));
        cx.spawn(async move |this, cx| {
            let error: Option<SharedString> = match call.await {
                Some(Ok(())) => return,
                Some(Err(error)) => Some(error.message.into()),
                None => Some(text::STOPPED.into()),
            };
            let _ = this.update(cx, |this, cx| this.operation_error(error, cx));
        })
        .detach();
    }

    /// Whether a row drag is in progress, so rows show its marks.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Ends what the pane knows of a drag once GPUI's has ended, as when
    /// it is let go outside the tree.
    pub(super) fn end_drag_if_over(&mut self, cx: &Context<Self>) {
        if self.drag.is_some() && !cx.has_active_drag() {
            self.drag = None;
        }
    }

    /// Where on row `index` a drop at `y` lands for `what`.
    fn place_at(
        &self,
        index: usize,
        what: &Dragged,
        top: f32,
        height: f32,
        y: f32,
    ) -> Option<DropPlace> {
        let item = self.rows.get(index)?.item()?;
        let fraction = if height > 0. { (y - top) / height } else { 0.5 };
        groups::drop_place(what, item, fraction)
    }

    /// The pointer moved during a drag: row `index` is the target while
    /// the pointer is on it and it takes the drop.
    fn drag_move(
        &mut self,
        index: usize,
        event: &DragMoveEvent<DraggedRow>,
        cx: &mut Context<Self>,
    ) {
        let dragged = event.drag(cx).clone();
        let Some(row) = self.rows.get(index).map(|row| row.id.clone()) else {
            return;
        };
        let bounds = event.bounds;
        let inside = bounds.contains(&event.event.position);
        let place = if inside {
            self.place_at(
                index,
                &dragged.what,
                f32::from(bounds.top()),
                f32::from(bounds.size.height),
                f32::from(event.event.position.y),
            )
        } else {
            None
        };
        let drag = self.drag.get_or_insert_with(Drag::default);
        let mut changed = drag.row.as_deref() != Some(dragged.row.as_str());
        drag.row = Some(dragged.row);
        let target = if inside {
            place.map(|place| (row, place))
        } else if drag.target.as_ref().is_some_and(|(at, _)| at == &row) {
            // The pointer left this row: it is no longer the target.
            None
        } else {
            drag.target.clone()
        };
        changed |= drag.target != target;
        drag.target = target;
        if changed {
            cx.notify();
        }
    }

    /// `dragged` was let go on row `index` (UI-7): Ctrl, or Alt as on
    /// macOS, adds instead of moving.
    fn drop_row(
        &mut self,
        index: usize,
        dragged: &DraggedRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drag = None;
        cx.notify();
        let bounds = self.row_bounds.borrow().get(&index).copied();
        let Some(bounds) = bounds else {
            return;
        };
        let place = self.place_at(
            index,
            &dragged.what,
            f32::from(bounds.top()),
            f32::from(bounds.size.height),
            f32::from(window.mouse_position().y),
        );
        let modifiers = window.modifiers();
        let copy = modifiers.control || modifiers.alt;
        let groups = groups::groups_of(self.tree.roots());
        let op = self
            .rows
            .get(index)
            .and_then(|row| row.item())
            .and_then(|item| groups::drop_op(&groups, &dragged.what, item, place?, copy));
        if let Some(op) = op {
            self.send_group_op(op, cx);
        }
    }

    /// Row `index`, with `id` and `label`, made draggable and a drop target,
    /// with the marks of a drag in progress (UI-23).
    pub(super) fn with_drag(
        &self,
        row: impl ParentElement + Styled + StatefulInteractiveElement + IntoElement,
        index: usize,
        id: &str,
        label: &str,
        item: &TreeItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tokens = *cx.tokens();
        let row_id = self.rows.get(index).map(|row| row.id.clone());
        let marks = self.drag.as_ref();
        let dragged = marks.is_some_and(|drag| drag.row.is_some() && drag.row == row_id);
        let place = marks
            .and_then(|drag| drag.target.as_ref())
            .filter(|(at, _)| Some(at) == row_id.as_ref())
            .map(|(_, place)| *place);
        let what = groups::dragged_of(item);
        row.when_some(what.zip(row_id), |row, (what, row_id)| {
            let value = DraggedRow {
                row: row_id,
                label: label.to_owned().into(),
                what,
            };
            row.on_drag(value, |dragged, _, _, cx| {
                cx.new(|_| DraggedRowView(dragged.label.clone()))
            })
        })
        .on_drag_move(
            cx.listener(move |this, event: &DragMoveEvent<DraggedRow>, _, cx| {
                this.drag_move(index, event, cx);
            }),
        )
        .on_drop(cx.listener(move |this, dragged: &DraggedRow, window, cx| {
            this.drop_row(index, dragged, window, cx);
        }))
        .when(dragged, |row| {
            row.child(
                div()
                    .id(ElementId::Name(format!("{id}-dragged").into()))
                    .test_support()
                    .absolute()
                    .inset_0()
                    .bg(tokens.bg_3)
                    .opacity(0.6),
            )
        })
        .when_some(place, |row, place| {
            let mark = div()
                .id(ElementId::Name(format!("{id}-drop-{}", place.id()).into()))
                .test_support()
                .absolute()
                .left_0()
                .right_0();
            row.child(match place {
                DropPlace::Into => mark
                    .top_0()
                    .bottom_0()
                    .border_1()
                    .border_color(tokens.accent)
                    .rounded(px(2.)),
                DropPlace::Before => mark.top_0().h(px(2.)).bg(tokens.accent),
                DropPlace::After => mark.bottom_0().h(px(2.)).bg(tokens.accent),
            })
        })
    }
}
