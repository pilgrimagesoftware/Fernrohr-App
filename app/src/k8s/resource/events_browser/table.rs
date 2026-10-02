//! The events browser's table: its delegate (column order, widths, sort, the
//! displayed rows) and the `DataTable` it is drawn with.

use super::columns::EventColumn;
use super::row::EventRow;
use crate::ui::table_fit::{FitColumns, HeaderBounds};
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};
use gpui_kit::*;
use jiff::Timestamp;

/// The sort an events browser opens with: Last Seen ascending, which is
/// newest first (`events-browser`: "Default sort is newest first").
pub const DEFAULT_SORT: (EventColumn, ColumnSort) = (EventColumn::LastSeen, ColumnSort::Ascending);

/// Behind the events table: the columns, their widths, the active sort, and the
/// rows in the order last supplied and in displayed order.
pub struct EventsTableDelegate {
    natural: Vec<EventRow>,
    rows: Vec<EventRow>,
    columns: Vec<EventColumn>,
    sort: Option<(EventColumn, ColumnSort)>,
    widths: Vec<(EventColumn, Pixels)>,
    /// The event this table last had selected, by uid, to re-point the
    /// highlight after a sort or a watch update moves it.
    selected: Option<String>,
    /// When the rows were supplied, which their Last Seen ages read against.
    now: Timestamp,
    header: HeaderBounds,
}

impl EventsTableDelegate {
    pub fn new(sort: Option<(EventColumn, ColumnSort)>) -> Self {
        Self {
            natural: Vec::new(),
            rows: Vec::new(),
            columns: EventColumn::DEFAULT_ORDER.to_vec(),
            sort,
            widths: Vec::new(),
            selected: None,
            now: Timestamp::now(),
            header: HeaderBounds::default(),
        }
    }

    /// Replaces the rows, keeping the active sort applied.
    pub fn set_rows(&mut self, rows: Vec<EventRow>, now: Timestamp) {
        self.natural = rows;
        self.now = now;
        self.apply_sort();
    }

    /// The rows in displayed order. Test-only: the panel reads the selection.
    #[cfg(test)]
    pub fn rows(&self) -> &[EventRow] {
        &self.rows
    }

    /// The active sort, as saved with the panel.
    pub fn sort(&self) -> Option<(EventColumn, ColumnSort)> {
        self.sort
    }

    pub fn remember_selection(&mut self, row_ix: usize) {
        self.selected = self.rows.get(row_ix).map(|row| row.uid.clone());
    }

    /// The selected event, if it is still listed.
    pub fn selected(&self) -> Option<&EventRow> {
        let uid = self.selected.as_deref()?;
        self.rows.iter().find(|row| row.uid == uid)
    }

    fn selected_index(&self) -> Option<usize> {
        let uid = self.selected.as_deref()?;
        self.rows.iter().position(|row| row.uid == uid)
    }

    fn width_of(&self, column: EventColumn) -> Pixels {
        self.widths
            .iter()
            .find(|(col, _)| *col == column)
            .map_or(px(column.default_width()), |(_, width)| *width)
    }

    fn set_width(&mut self, column: EventColumn, width: Pixels) {
        match self.widths.iter_mut().find(|(col, _)| *col == column) {
            Some((_, current)) => *current = width,
            None => self.widths.push((column, width)),
        }
    }

    /// Records the widths of every column, left to right - the table's
    /// `ColumnWidthsChanged`.
    pub fn set_widths(&mut self, widths: &[Pixels]) {
        for (column, width) in self.columns.clone().into_iter().zip(widths) {
            self.set_width(column, *width);
        }
    }

    fn apply_sort(&mut self) {
        self.rows = self.natural.clone();
        match self.sort {
            Some((col, ColumnSort::Ascending)) => self.rows.sort_by(|a, b| col.compare(a, b)),
            Some((col, ColumnSort::Descending)) => self.rows.sort_by(|a, b| col.compare(b, a)),
            Some((_, ColumnSort::Default)) | None => {}
        }
    }
}

impl TableDelegate for EventsTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
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

    /// A Warning's type in the warning tone; every cell in the data font.
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        use crate::ui::style::{self, Tone};
        use crate::ui::typography::TypeRole as _;
        let row = &self.rows[row_ix];
        let column = self.columns[col_ix];
        let cell = div()
            .debug_selector(|| format!("event-cell-{row_ix}-{col_ix}"))
            .data_font()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis();
        let text = column.text(row, self.now);
        match column {
            EventColumn::Type if row.type_.as_deref() == Some("Warning") => cell
                .text_color(style::status(Tone::Warning, cx))
                .child(text),
            _ => cell.child(text),
        }
    }

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
        self.columns[col_ix].text(&self.rows[row_ix], self.now)
    }

    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        let column = self.columns.remove(col_ix);
        self.columns.insert(to_ix, column);
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.sort = Some((self.columns[col_ix], sort));
        self.apply_sort();
        // Mid-update on the table here, so re-point the selection once it's free.
        cx.defer_in(window, |table, _window, cx| reselect(table, cx));
    }
}

impl FitColumns for EventsTableDelegate {
    fn header_bounds(&self) -> &HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get(col_ix).copied() {
            self.set_width(column, width);
        }
    }
}

/// Re-points the selection at the event this table last selected, wherever a
/// sort or update moved it; clears it if the event has expired.
pub fn reselect(
    table: &mut TableState<EventsTableDelegate>,
    cx: &mut Context<TableState<EventsTableDelegate>>,
) {
    if table.delegate().selected.is_none() {
        return;
    }
    match table.delegate().selected_index() {
        Some(row_ix) if table.selected_row() != Some(row_ix) => table.set_selected_row(row_ix, cx),
        Some(_) => {}
        None if table.selected_row().is_some() => table.clear_selection(cx),
        None => {}
    }
}

/// The table over `state`, drawn like the other list panels' - striped,
/// bordered, scrollable both ways - with a header divider double-click
/// fitting a column.
pub fn data_table(
    state: &Entity<TableState<EventsTableDelegate>>,
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
