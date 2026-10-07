//! The Pods table's column model, comparator, and [`gpui_kit`] table delegate -
//! split out of `pods.rs` to keep that file under the 700-line limit. This
//! module owns *what a column is* (identity, title, default width, how two
//! rows compare on it) and *how the table renders/sorts/reorders* from that;
//! `pods.rs` owns the panel around it (the watch, actions, layout).

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};
use gpui_kit::*;

use super::pods::{PodRow, PodSelection};

mod columns;
use columns::cell_text;
pub(super) use columns::{PodColumn, compare};

/// One row of the live Pods table: the display fields plus the selection a
/// click on it should publish.
#[derive(Clone)]
pub(super) struct PodTableRow {
    pub(super) row: PodRow,
    pub(super) selection: PodSelection,
    /// The forwards reaching the pod, for the Forwards cell's tooltip.
    pub(super) forwards: Vec<crate::k8s::cluster::port_forwards::ForwardSummary>,
}

/// The Pods panel's table over `state`: striped, bordered, scrollable both
/// ways, its rows `ui::space`'s row height, with a double-click on a header
/// divider fitting the column on its left.
pub(super) fn data_table(
    state: &Entity<TableState<PodTableDelegate>>,
    cx: &App,
) -> impl IntoElement + use<> {
    use gpui_kit::component::Sizable as _;
    let size = crate::ui::table_fit::table_size(cx);
    div()
        .size_full()
        .relative()
        .child(
            DataTable::new(state)
                .stripe(true)
                .bordered(true)
                .scrollbar_visible(true, true)
                .with_size(size),
        )
        .child(crate::ui::table_fit::divider_double_click(state, size))
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
    /// Widths the user set, by dragging a divider or double-clicking one, per
    /// column; a column without one is [`PodColumn::default_width`] wide. Kept
    /// by column rather than by position, so a reordered column keeps its own.
    widths: Vec<(PodColumn, Pixels)>,
    /// The header cells' drawn bounds, for a divider double-click to fit a
    /// column (`ui::table_fit`).
    header: crate::ui::table_fit::HeaderBounds,
    /// The open quick look, drawn beside the selected row (`pod-quick-look`).
    quick_look: Option<Entity<super::pods::quick_look::QuickLookPopover>>,
    /// The table's own focus handle, which the row context menu dispatches
    /// its commands from so they reach the Pods panel as their keys do.
    action_context: Option<FocusHandle>,
    background_click: crate::ui::background_rows::BackgroundClick,
}

impl Default for PodTableDelegate {
    fn default() -> Self {
        Self {
            natural: Vec::new(),
            rows: Vec::new(),
            columns: PodColumn::DEFAULT_ORDER.to_vec(),
            sort: None,
            selected: None,
            widths: Vec::new(),
            header: Default::default(),
            quick_look: None,
            action_context: None,
            background_click: Default::default(),
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

    /// `column`'s width: the one the user set, or its default.
    fn width_of(&self, column: PodColumn) -> Pixels {
        self.widths
            .iter()
            .find(|(col, _)| *col == column)
            .map_or(px(column.default_width()), |(_, width)| *width)
    }

    fn set_width(&mut self, column: PodColumn, width: Pixels) {
        match self.widths.iter_mut().find(|(col, _)| *col == column) {
            Some((_, current)) => *current = width,
            None => self.widths.push((column, width)),
        }
    }

    /// Records new widths for every column, left to right - the table's
    /// `ColumnWidthsChanged`, so a later `refresh` (a divider double-click's)
    /// keeps what the user dragged.
    pub(super) fn set_widths(&mut self, widths: &[Pixels]) {
        for (column, width) in self.columns.clone().into_iter().zip(widths) {
            self.set_width(column, *width);
        }
    }

    /// The rows in their currently displayed order.
    pub(super) fn set_quick_look(
        &mut self,
        popover: Option<Entity<super::pods::quick_look::QuickLookPopover>>,
    ) {
        self.quick_look = popover;
    }

    pub(super) fn set_action_context(&mut self, focus: FocusHandle) {
        self.action_context = Some(focus);
    }

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
    #[cfg(test)]
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

impl crate::ui::background_rows::BackgroundRows for PodTableDelegate {
    fn open_in_background(&mut self, row_ix: usize, window: &mut Window, cx: &mut App) {
        if let Some(row) = self.rows.get(row_ix) {
            window.dispatch_action(Box::new(row.selection.background_open()), cx);
        }
    }

    fn background_click(&mut self) -> &mut crate::ui::background_rows::BackgroundClick {
        &mut self.background_click
    }
}

impl TableDelegate for PodTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        crate::ui::background_rows::row(row_ix, cx)
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let col = self.columns[col_ix];
        let column = Column::new(col.id(), col.title())
            .width(self.width_of(col))
            .sortable();
        match self.sort {
            Some((active, sort)) if active == col => column.sort(sort),
            _ => column,
        }
    }

    /// The row's context menu (`pod-quick-look` 2.1): right-clicking selects
    /// the row - deferred, since the table is mid-update here - and offers the
    /// panel's registered pod commands, dispatched from the table so each runs
    /// exactly as its key does, with that key shown beside it.
    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        use super::pods::{DescribePod, QuickLook, ShowPodLogs, ShowPodLogsFlipped, ShowPodYaml};
        if row_ix >= self.rows.len() {
            return menu;
        }
        cx.defer_in(window, move |table, _window, cx| {
            table.set_selected_row(row_ix, cx);
        });
        let menu = match &self.action_context {
            Some(focus) => menu.action_context(focus.clone()),
            None => menu,
        };
        menu.menu("Quick Look", Box::new(QuickLook))
            .menu("Open Details", Box::new(DescribePod))
            .menu("Logs", Box::new(ShowPodLogs))
            .menu(
                crate::ui::logs_panels::flipped_label(cx),
                Box::new(ShowPodLogsFlipped),
            )
            .menu("YAML", Box::new(ShowPodYaml))
    }

    /// Status in its tone's colour, Ready with a dot in its readiness tone,
    /// and a non-zero restart count in its restart tone - the tones decided by
    /// the row projection (`pods::rows`), mapped to colour by `ui::style`.
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        use crate::ui::style::{self, Tone};
        use crate::ui::typography::TypeRole as _;
        let row = &self.rows[row_ix].row;
        let column = self.columns[col_ix];
        let text = cell_text(row, column);
        let cell = div()
            .debug_selector(|| format!("pod-cell-{row_ix}-{col_ix}"))
            .data_font()
            .whitespace_nowrap();
        let cell = match column {
            PodColumn::Status => cell
                .text_color(style::status(row.status_tone, cx))
                .child(text),
            PodColumn::Ready => cell
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(style::status(row.ready_tone, cx)),
                )
                .child(text),
            PodColumn::Restarts if row.restart_tone != Tone::Neutral => cell
                .text_color(style::status(row.restart_tone, cx))
                .child(text),
            PodColumn::Forwards => cell.children(crate::ui::forward_indicator::indicator(
                &row.name,
                &self.rows[row_ix].forwards,
            )),
            _ => cell.child(text),
        };
        // An open quick look hangs below the selected row's first cell, in an
        // overlay so the row keeps its height and the popover isn't clipped.
        let popover = self.quick_look.clone().filter(|_| {
            col_ix == 0 && self.selected.as_ref() == Some(&self.rows[row_ix].selection)
        });
        match popover {
            Some(popover) => div()
                .child(cell)
                .child(
                    deferred(
                        anchored()
                            .snap_to_window_with_margin(px(8.))
                            .child(div().pt_1().child(popover)),
                    )
                    .with_priority(1),
                )
                .into_any_element(),
            None => cell.into_any_element(),
        }
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

    /// The column's title, as the default draws it, with its bounds recorded
    /// for the divider double-click.
    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let title = self.columns[col_ix].title();
        self.header.track(col_ix, div().size_full().child(title))
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _cx: &App) -> String {
        cell_text(&self.rows[row_ix].row, self.columns[col_ix])
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

impl crate::ui::table_fit::FitColumns for PodTableDelegate {
    fn header_bounds(&self) -> &crate::ui::table_fit::HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get(col_ix).copied() {
            self.set_width(column, width);
        }
    }
}
