//! The history's column order and widths (UI-36), a port of rc.1's
//! `stores/layout.ts` columns, without a UI toolkit. The subject takes the
//! width that is left, so only the other four have widths. The history pane
//! keeps one of these per workspace, read from and saved to its pane layout
//! (ARCH-9, UI-29).

use gitcroc_core::{ColumnLayout, ColumnOrder, HistoryColumn};

/// The smallest width of a column with one of its own, in CSS pixels.
pub const MIN_COLUMN_WIDTH: u32 = 32;
/// What a resize leaves the subject at least, in CSS pixels.
pub const MIN_SUBJECT_WIDTH: f32 = 80.;
/// How far an arrow key resizes a focused edge.
pub const KEY_STEP: i32 = 16;

pub const DEFAULT_GRAPH_WIDTH: u32 = 80;
pub const DEFAULT_AUTHOR_WIDTH: u32 = 140;
pub const DEFAULT_DATE_WIDTH: u32 = 128;
pub const DEFAULT_HASH_WIDTH: u32 = 72;

/// A history column with a width of its own: every one but the subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizedColumn {
    Graph,
    Author,
    Date,
    Hash,
}

impl SizedColumn {
    pub const ALL: [SizedColumn; 4] = [
        SizedColumn::Graph,
        SizedColumn::Author,
        SizedColumn::Date,
        SizedColumn::Hash,
    ];

    pub fn column(self) -> HistoryColumn {
        match self {
            SizedColumn::Graph => HistoryColumn::Graph,
            SizedColumn::Author => HistoryColumn::Author,
            SizedColumn::Date => HistoryColumn::Date,
            SizedColumn::Hash => HistoryColumn::Hash,
        }
    }

    pub fn from_column(column: HistoryColumn) -> Option<Self> {
        match column {
            HistoryColumn::Graph => Some(SizedColumn::Graph),
            HistoryColumn::Author => Some(SizedColumn::Author),
            HistoryColumn::Date => Some(SizedColumn::Date),
            HistoryColumn::Hash => Some(SizedColumn::Hash),
            HistoryColumn::Subject => None,
        }
    }

    pub fn default_width(self) -> u32 {
        match self {
            SizedColumn::Graph => DEFAULT_GRAPH_WIDTH,
            SizedColumn::Author => DEFAULT_AUTHOR_WIDTH,
            SizedColumn::Date => DEFAULT_DATE_WIDTH,
            SizedColumn::Hash => DEFAULT_HASH_WIDTH,
        }
    }

    fn index(self) -> usize {
        match self {
            SizedColumn::Graph => 0,
            SizedColumn::Author => 1,
            SizedColumn::Date => 2,
            SizedColumn::Hash => 3,
        }
    }
}

/// The order and widths of one workspace's history columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Columns {
    order: Vec<HistoryColumn>,
    widths: [u32; 4],
    active: HistoryColumn,
}

impl Default for Columns {
    fn default() -> Self {
        let order = HistoryColumn::DEFAULT_ORDER.to_vec();
        let active = order.first().copied().unwrap_or(HistoryColumn::Graph);
        Self {
            order,
            widths: [
                DEFAULT_GRAPH_WIDTH,
                DEFAULT_AUTHOR_WIDTH,
                DEFAULT_DATE_WIDTH,
                DEFAULT_HASH_WIDTH,
            ],
            active,
        }
    }
}

impl Columns {
    /// The workspace's saved columns, or the defaults (UI-36). A width the
    /// file lost or set too small gets its minimum.
    pub fn from_layout(saved: Option<&ColumnLayout>) -> Self {
        let mut columns = Self::default();
        let Some(saved) = saved else {
            return columns;
        };
        if let Some(ColumnOrder(saved_order)) = &saved.order {
            let mut order = Vec::with_capacity(columns.order.len());
            for column in saved_order.iter().chain(&HistoryColumn::DEFAULT_ORDER) {
                if !order.contains(column) {
                    order.push(*column);
                }
            }
            columns.order = order;
        }
        for (sized, saved) in [
            (SizedColumn::Graph, saved.graph),
            (SizedColumn::Author, saved.author),
            (SizedColumn::Date, saved.date),
            (SizedColumn::Hash, saved.hash),
        ] {
            if let Some(width) = saved {
                columns.widths[sized.index()] = width.max(MIN_COLUMN_WIDTH);
            }
        }
        columns.active = columns
            .order
            .first()
            .copied()
            .unwrap_or(HistoryColumn::Graph);
        columns
    }

    pub fn order(&self) -> &[HistoryColumn] {
        &self.order
    }

    pub fn active(&self) -> HistoryColumn {
        self.active
    }

    pub fn set_active(&mut self, column: HistoryColumn) {
        if self.order.contains(&column) {
            self.active = column;
        }
    }

    pub fn first(&self) -> HistoryColumn {
        self.order.first().copied().unwrap_or(HistoryColumn::Graph)
    }

    pub fn last(&self) -> HistoryColumn {
        self.order.last().copied().unwrap_or(HistoryColumn::Graph)
    }

    pub fn position(&self, column: HistoryColumn) -> Option<usize> {
        self.order.iter().position(|other| *other == column)
    }

    pub fn width(&self, column: HistoryColumn) -> Option<u32> {
        SizedColumn::from_column(column).map(|sized| self.width_of(sized))
    }

    pub fn width_of(&self, sized: SizedColumn) -> u32 {
        self.widths[sized.index()]
    }

    /// The widest `sized` can get while the subject keeps its minimum.
    pub fn max_width(&self, sized: SizedColumn, subject_width: f32) -> u32 {
        let extra = (subject_width - MIN_SUBJECT_WIDTH).max(0.).floor();
        self.width_of(sized)
            .saturating_add(extra as u32)
            .max(MIN_COLUMN_WIDTH)
    }

    /// Sets `sized` to `width`, at most what the subject can give up.
    pub fn set_width(&mut self, sized: SizedColumn, width: f32, subject_width: f32) -> bool {
        let max = self.max_width(sized, subject_width);
        self.set_width_capped(sized, width, max)
    }

    /// Sets `sized` to `width`, at most `max`: a drag keeps the maximum it
    /// began with, as the subject's width is laid out again each move.
    pub fn set_width_capped(&mut self, sized: SizedColumn, width: f32, max: u32) -> bool {
        let width =
            (width.round().max(0.) as u32).clamp(MIN_COLUMN_WIDTH, max.max(MIN_COLUMN_WIDTH));
        let slot = &mut self.widths[sized.index()];
        if *slot == width {
            return false;
        }
        *slot = width;
        true
    }

    /// Steps `sized` by `delta`, keeping the subject's minimum.
    pub fn resize(&mut self, sized: SizedColumn, delta: i32, subject_width: f32) -> bool {
        let width = (self.width_of(sized) as i32).saturating_add(delta) as f32;
        self.set_width(sized, width, subject_width)
    }

    /// Gives `sized` its default width.
    pub fn reset_width(&mut self, sized: SizedColumn) -> bool {
        let slot = &mut self.widths[sized.index()];
        let default = sized.default_width();
        if *slot == default {
            return false;
        }
        *slot = default;
        true
    }

    /// Moves `column` to place `to` of the order.
    pub fn move_column(&mut self, column: HistoryColumn, to: usize) -> bool {
        let Some(from) = self.position(column) else {
            return false;
        };
        if from == to {
            return false;
        }
        self.order.remove(from);
        let to = to.min(self.order.len());
        self.order.insert(to, column);
        true
    }

    /// Moves `column` one place to the left, when it is not first.
    pub fn move_left(&mut self, column: HistoryColumn) -> bool {
        match self.position(column) {
            Some(at) if at > 0 => self.move_column(column, at - 1),
            _ => false,
        }
    }

    /// Moves `column` one place to the right, when it is not last.
    pub fn move_right(&mut self, column: HistoryColumn) -> bool {
        match self.position(column) {
            Some(at) if at + 1 < self.order.len() => self.move_column(column, at + 1),
            _ => false,
        }
    }

    /// The default order and widths (UI-36), with the header that has the
    /// focus still active.
    pub fn reset(&mut self) {
        let active = self.active;
        *self = Self::default();
        self.active = active;
    }

    /// Whether the columns are the defaults, so the file needs none.
    pub fn is_default(&self) -> bool {
        self.order == HistoryColumn::DEFAULT_ORDER
            && self.widths
                == [
                    DEFAULT_GRAPH_WIDTH,
                    DEFAULT_AUTHOR_WIDTH,
                    DEFAULT_DATE_WIDTH,
                    DEFAULT_HASH_WIDTH,
                ]
    }

    /// What the workspace file keeps, or `None` while they are the
    /// defaults: Reset columns removes them from the file (UI-36).
    pub fn layout(&self) -> Option<ColumnLayout> {
        if self.is_default() {
            return None;
        }
        Some(ColumnLayout {
            order: Some(ColumnOrder(self.order.clone())),
            graph: Some(self.width_of(SizedColumn::Graph)),
            author: Some(self.width_of(SizedColumn::Author)),
            date: Some(self.width_of(SizedColumn::Date)),
            hash: Some(self.width_of(SizedColumn::Hash)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_36_the_defaults_are_the_rc_1_order_and_widths() {
        let columns = Columns::default();
        assert_eq!(columns.order(), HistoryColumn::DEFAULT_ORDER);
        assert_eq!(columns.width(HistoryColumn::Graph), Some(80));
        assert_eq!(columns.width(HistoryColumn::Author), Some(140));
        assert_eq!(columns.width(HistoryColumn::Date), Some(128));
        assert_eq!(columns.width(HistoryColumn::Hash), Some(72));
        assert_eq!(columns.width(HistoryColumn::Subject), None);
        assert_eq!(columns.layout(), None);
    }

    #[test]
    fn ui_36_saved_columns_read_leniently_and_small_widths_get_the_minimum() {
        let columns = Columns::from_layout(Some(&ColumnLayout {
            order: Some(ColumnOrder(vec![
                HistoryColumn::Hash,
                HistoryColumn::Hash,
                HistoryColumn::Subject,
            ])),
            graph: Some(12),
            author: None,
            date: Some(100),
            hash: Some(90),
        }));
        assert_eq!(
            columns.order(),
            [
                HistoryColumn::Hash,
                HistoryColumn::Subject,
                HistoryColumn::Graph,
                HistoryColumn::Author,
                HistoryColumn::Date,
            ]
        );
        assert_eq!(columns.width(HistoryColumn::Graph), Some(32));
        assert_eq!(columns.width(HistoryColumn::Author), Some(140));
        assert_eq!(columns.width(HistoryColumn::Date), Some(100));
        assert_eq!(columns.width(HistoryColumn::Hash), Some(90));
    }

    #[test]
    fn ui_36_resizing_stops_at_the_subjects_minimum() {
        let mut columns = Columns::default();
        assert!(columns.resize(SizedColumn::Author, 40, 120.));
        assert_eq!(columns.width_of(SizedColumn::Author), 180);
        assert!(!columns.resize(SizedColumn::Author, 1000, 80.));
        assert_eq!(columns.width_of(SizedColumn::Author), 180);
        assert!(columns.set_width(SizedColumn::Author, 1000., 160.));
        assert_eq!(columns.width_of(SizedColumn::Author), 260);
    }

    #[test]
    fn ui_36_moving_and_resetting_columns() {
        let mut columns = Columns::default();
        assert!(columns.move_column(HistoryColumn::Hash, 0));
        assert_eq!(columns.order()[0], HistoryColumn::Hash);
        assert!(!columns.move_left(HistoryColumn::Hash));
        assert!(columns.move_right(HistoryColumn::Hash));
        assert_eq!(columns.order()[1], HistoryColumn::Hash);
        columns.set_active(HistoryColumn::Hash);
        columns.reset();
        assert_eq!(columns.order(), HistoryColumn::DEFAULT_ORDER);
        assert_eq!(columns.active(), HistoryColumn::Hash);
        assert_eq!(columns.layout(), None);
    }
}
