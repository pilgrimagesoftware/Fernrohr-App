//! The Pods table's column model, comparator, and [`gpui_kit`] table delegate -
//! split out of `pods.rs` to keep that file under the 700-line limit. This
//! module owns *what a column is* (identity, title, default width, how two
//! rows compare on it) and *how the table renders/sorts/reorders* from that;
//! `pods.rs` owns the panel around it (the watch, actions, layout).

use std::cmp::Ordering;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::*;

use super::pods::{PodRow, PodSelection};

/// One column of the Pods table. A closed enum rather than a string/index
/// pair: a `match` on it has no wildcard arm, so a new variant fails to
/// compile everywhere it isn't handled instead of silently falling back to
/// "Name" the way the old string-keyed columns did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PodColumn {
    Name,
    Namespace,
    Ready,
    Status,
    Restarts,
    Age,
    Ip,
    Node,
}

impl PodColumn {
    /// Left-to-right order a freshly created table starts with.
    pub(super) const DEFAULT_ORDER: [PodColumn; 8] = [
        PodColumn::Name,
        PodColumn::Namespace,
        PodColumn::Ready,
        PodColumn::Status,
        PodColumn::Restarts,
        PodColumn::Age,
        PodColumn::Ip,
        PodColumn::Node,
    ];

    /// The column's stable id, used as both the `gpui_kit` [`Column`] key and
    /// [`crate::config::workspace::SortState::column`]'s persisted value.
    fn id(self) -> &'static str {
        match self {
            PodColumn::Name => "name",
            PodColumn::Namespace => "namespace",
            PodColumn::Ready => "ready",
            PodColumn::Status => "status",
            PodColumn::Restarts => "restarts",
            PodColumn::Age => "age",
            PodColumn::Ip => "ip",
            PodColumn::Node => "node",
        }
    }

    fn title(self) -> &'static str {
        match self {
            PodColumn::Name => "Name",
            PodColumn::Namespace => "Namespace",
            PodColumn::Ready => "Ready",
            PodColumn::Status => "Status",
            PodColumn::Restarts => "Restarts",
            PodColumn::Age => "Age",
            PodColumn::Ip => "IP",
            PodColumn::Node => "Node",
        }
    }

    fn default_width(self) -> f32 {
        match self {
            PodColumn::Name => 220.,
            PodColumn::Namespace => 150.,
            PodColumn::Ready => 80.,
            PodColumn::Status => 130.,
            PodColumn::Restarts => 90.,
            PodColumn::Age => 70.,
            PodColumn::Ip => 150.,
            PodColumn::Node => 180.,
        }
    }

    /// The column an id names, or [`PodColumn::Name`] for an id this build
    /// doesn't know - e.g. a [`crate::config::workspace::SortState`] persisted
    /// by an older version. Kept as a lookup over [`Self::id`] rather than a
    /// second parallel `match`, so the two can't drift.
    pub(super) fn from_id(id: &str) -> Self {
        Self::DEFAULT_ORDER
            .into_iter()
            .find(|col| col.id() == id)
            .unwrap_or(PodColumn::Name)
    }
}

/// The pure per-column comparison, shared by [`PodTableDelegate`]'s
/// interactive sort and `pods::sort_rows`'s view-pipeline sort - one
/// definition of "how does column X order two rows" for both.
pub(super) fn compare(a: &PodRow, b: &PodRow, col: PodColumn) -> Ordering {
    match col {
        PodColumn::Name => a.name.cmp(&b.name),
        PodColumn::Namespace => a.namespace.cmp(&b.namespace),
        PodColumn::Ready => a.ready.cmp(&b.ready),
        PodColumn::Status => a.status.cmp(&b.status),
        // Raw seconds, not the display string: see `PodRow::age_secs`.
        PodColumn::Age => a.age_secs.cmp(&b.age_secs),
        PodColumn::Restarts => a.restarts.cmp(&b.restarts),
        PodColumn::Ip => a.pod_ip.cmp(&b.pod_ip),
        PodColumn::Node => a.node.cmp(&b.node),
    }
}

/// A column's rendered text for one row - the pure half of `render_td`.
fn cell_text(row: &PodRow, col: PodColumn) -> String {
    match col {
        PodColumn::Name => row.name.clone(),
        PodColumn::Namespace => row.namespace.clone(),
        PodColumn::Ready => row.ready.clone(),
        PodColumn::Status => row.status.clone(),
        PodColumn::Restarts => row.restarts.to_string(),
        PodColumn::Age => row.age.clone(),
        PodColumn::Ip => row.pod_ip.clone(),
        PodColumn::Node => row.node.clone(),
    }
}

/// One row of the live Pods table: the display fields plus the selection a
/// click on it should publish.
#[derive(Clone)]
pub(super) struct PodTableRow {
    pub(super) row: PodRow,
    pub(super) selection: PodSelection,
}

/// The [`TableDelegate`] backing a Pods panel's table: owns the column order,
/// the active sort, and the rows in both their natural (as last supplied by
/// `PodsPanel::sync_table`) and currently-displayed order.
pub(super) struct PodTableDelegate {
    /// Rows in the order `sync_table` last supplied them. Kept aside (rather
    /// than sorting `rows` in place and losing this), so a `ColumnSort::Default`,
    /// or simply new rows arriving, can restore this order without the caller
    /// having to resupply it.
    natural: Vec<PodTableRow>,
    /// `natural`, permuted by `sort` if any. What `rows_count`/`render_td`/
    /// `context_menu`/row-selection all index.
    rows: Vec<PodTableRow>,
    columns: Vec<PodColumn>,
    sort: Option<(PodColumn, ColumnSort)>,
    /// The pod this table last had selected, by identity. [`TableState`]
    /// only keeps the selected row's index, and the app-wide `SelectedPod`
    /// global is shared by every Pods panel, so each table remembers its own
    /// pick to re-point its highlight after a sort or row update.
    selected: Option<PodSelection>,
}

impl Default for PodTableDelegate {
    fn default() -> Self {
        Self {
            natural: Vec::new(),
            rows: Vec::new(),
            columns: PodColumn::DEFAULT_ORDER.to_vec(),
            sort: None,
            selected: None,
        }
    }
}

impl PodTableDelegate {
    /// Replaces the rows, keeping the active sort applied - the delegate's
    /// half of `PodsPanel::sync_table`'s per-render row refresh.
    pub(super) fn set_rows(&mut self, rows: Vec<PodTableRow>) {
        self.natural = rows;
        self.apply_sort();
    }

    /// The rows in their currently displayed order.
    pub(super) fn rows(&self) -> &[PodTableRow] {
        &self.rows
    }

    /// Records the pod the user selected in this table (see `selected`).
    pub(super) fn remember_selection(&mut self, selection: Option<PodSelection>) {
        self.selected = selection;
    }

    /// The displayed row (if any) that identifies as `selection` - matched by
    /// namespace and name, not position. A pure query so tests can exercise
    /// the lookup [`reselect`] drives without a GPUI `Window`/`Context`.
    pub(super) fn index_of(&self, selection: &PodSelection) -> Option<usize> {
        self.rows.iter().position(|row| {
            row.selection.namespace == selection.namespace && row.selection.name == selection.name
        })
    }

    /// The text `render_td` shows at `(row_ix, col_ix)` - a pure query so
    /// tests can exercise the same lookup `render_td` renders from without a
    /// GPUI `Window`/`Context`.
    pub(super) fn cell_text_at(&self, row_ix: usize, col_ix: usize) -> String {
        cell_text(&self.rows[row_ix].row, self.columns[col_ix])
    }

    /// Moves the column at `col_ix` to `to_ix`, matching
    /// [`TableDelegate::move_column`]'s contract.
    pub(super) fn reorder_columns(&mut self, col_ix: usize, to_ix: usize) {
        let col = self.columns.remove(col_ix);
        self.columns.insert(to_ix, col);
    }

    /// Sets the active sort for the column at `col_ix` and re-derives `rows`
    /// from `natural`, matching [`TableDelegate::perform_sort`]'s contract:
    /// `sort` is the *new* direction to apply, already cycled by the caller.
    pub(super) fn resort(&mut self, col_ix: usize, sort: ColumnSort) {
        self.sort = Some((self.columns[col_ix], sort));
        self.apply_sort();
    }

    /// Rebuilds `rows` from `natural`: unchanged for `ColumnSort::Default` (or
    /// no active sort), ascending/descending otherwise. Always starts from
    /// `natural` - never re-sorts `rows` in place - so `Default` genuinely
    /// restores the incoming order rather than merely stopping further sorts.
    fn apply_sort(&mut self) {
        self.rows = self.natural.clone();
        match self.sort {
            Some((col, ColumnSort::Ascending)) => {
                self.rows.sort_by(|a, b| compare(&a.row, &b.row, col));
            }
            Some((col, ColumnSort::Descending)) => {
                // A reversed comparator, not `.reverse()` on the slice: with
                // ties, `.reverse()` would also reverse their relative order,
                // which is not what "descending" means for a stable sort.
                self.rows.sort_by(|a, b| compare(&b.row, &a.row, col));
            }
            Some((_, ColumnSort::Default)) | None => {}
        }
    }
}

impl TableDelegate for PodTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let col = self.columns[col_ix];
        let column = Column::new(col.id(), col.title())
            .width(px(col.default_width()))
            .sortable();
        match self.sort {
            Some((active, sort)) if active == col => column.sort(sort),
            _ => column,
        }
    }

    /// Section 5.2: a row's "Open" is the mouse-reachable twin of the `d`
    /// keybinding - both select the pod and emit `ShowPodDetail`, so they land
    /// on the same `open_target` call. Selecting here (not only dispatching)
    /// is what lets the menu act on the row under the pointer rather than
    /// whatever was selected last.
    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(selection) = self.rows.get(row_ix).map(|row| row.selection.clone()) else {
            return menu;
        };
        menu.item(
            PopupMenuItem::new("Open").on_click(move |_event, window, cx| {
                cx.set_global(super::pods::SelectedPod(Some(selection.clone())));
                window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
            }),
        )
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .whitespace_nowrap()
            .child(self.cell_text_at(row_ix, col_ix))
    }

    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        self.reorder_columns(col_ix, to_ix);
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.resort(col_ix, sort);
        // `TableState::perform_sort` (the caller) is already mid-update on
        // itself to reach this method, so `cx` here is only
        // `Context<TableState<Self>>` - there's no `&mut TableState<Self>` to
        // call `set_selected_row`/`clear_selection` on. Defer to the end of
        // this update cycle, when the entity is free again, to re-point the
        // selection at the pod this table selected instead of whatever pod
        // the sort left at the previously selected index.
        cx.defer_in(window, |table, _window, cx| reselect(table, cx));
    }
}

/// Re-points a Pods table's row *selection* - which [`TableState`] tracks as
/// a bare index - at the pod this table last selected, after something
/// (a sort, a watch-driven row replacement) has changed what occupies each
/// index. Moves the highlight if the pod moved, clears it if the pod is no
/// longer present, and leaves it alone if nothing is selected or the pod's
/// row didn't move.
pub(super) fn reselect(
    table: &mut TableState<PodTableDelegate>,
    cx: &mut Context<TableState<PodTableDelegate>>,
) {
    let Some(selection) = table.delegate().selected.clone() else {
        return;
    };
    match table.delegate().index_of(&selection) {
        Some(row_ix) if table.selected_row() != Some(row_ix) => table.set_selected_row(row_ix, cx),
        Some(_) => {}
        None if table.selected_row().is_some() => table.clear_selection(cx),
        None => {}
    }
}

#[cfg(test)]
mod tests;
