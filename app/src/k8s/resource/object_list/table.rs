//! A list panel's table: the [`gpui_kit`] table delegate that renders, sorts
//! and reorders a list's rows. What a column and a row *are*, and how two
//! rows compare on one, is [`list_model`]'s - split out to keep this file
//! under the 500-line limit.

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};
use gpui_kit::*;

mod list_model;
pub(super) use list_model::{ColumnLayout, ListColumn, ListRow, SavedColumn, apply_layout};
use list_model::{FORWARDS, NAME, cell_text, compare};

use super::columns;

/// Every one of `columns`' text for `row` - a list panel's "visible columns"
/// for its search box (`crate::ui::list_search`, #189), gathered the same way
/// `render_td`/`cell_text` reads each cell.
pub(super) fn visible_texts(row: &ListRow, columns: &[ListColumn]) -> Vec<String> {
    columns
        .iter()
        .map(|column| cell_text(row, column))
        .collect()
}

/// The panel's table over `state`: striped, bordered, scrollable both ways, its
/// rows `ui::space`'s row height - the same table the Pods panel draws - with a
/// double-click on a header divider fitting the column on its left.
pub(super) fn data_table(
    state: &Entity<TableState<ObjectTableDelegate>>,
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

/// What a row's "Open" calls with the row it was raised on.
type OpenRow = std::rc::Rc<dyn Fn(usize, &mut Window, &mut App)>;
/// Opens a row's object in the background, from the row itself - the table is
/// mid-update when a row is clicked, so the panel can't read it back.
type OpenRowInBackground = std::rc::Rc<dyn Fn(&ListRow, &mut Window, &mut App)>;

/// The [`TableDelegate`] behind a list panel's table: the column order and
/// widths, the active sort, and the rows in their natural and displayed order.
pub(super) struct ObjectTableDelegate {
    /// Rows as the panel last supplied them, kept so `ColumnSort::Default` (or new
    /// rows arriving) restores this order rather than leaving the last sort's.
    natural: Vec<ListRow>,
    /// `natural` permuted by `sort`: what rendering and selection index.
    rows: Vec<ListRow>,
    columns: Vec<ListColumn>,
    sort: Option<(SharedString, ColumnSort)>,
    /// What a third header click or Cycle Sort returns to: the kind's first
    /// column in its default order, ascending, whatever order the user has
    /// dragged the columns into.
    default_sort: crate::ui::list_sort::Sort,
    /// The key the user's sort is remembered under; `None` remembers nothing.
    remember_as: Option<String>,
    /// The object this table last had selected, by identity, to re-point its
    /// highlight after a sort or a watch update moves it.
    selected: Option<(Option<String>, String)>,
    /// Asks the panel to open the row a context menu was raised on.
    on_open: Option<OpenRow>,
    /// What a modified or middle click on a row does with it.
    on_open_in_background: Option<OpenRowInBackground>,
    background_click: crate::ui::background_rows::BackgroundClick,
    /// The header cells' drawn bounds, for a divider double-click to fit a
    /// column (`ui::table_fit`).
    header: crate::ui::table_fit::HeaderBounds,
}

impl ObjectTableDelegate {
    pub(super) fn new(columns: Vec<ListColumn>) -> Self {
        let default_sort = (
            columns
                .first()
                .map_or_else(|| SharedString::from(NAME), |column| column.id.clone()),
            false,
        );
        Self {
            natural: Vec::new(),
            rows: Vec::new(),
            columns,
            sort: None,
            default_sort,
            remember_as: None,
            selected: None,
            on_open: None,
            on_open_in_background: None,
            background_click: Default::default(),
            header: Default::default(),
        }
    }

    /// Sets the sort a third click returns to, and the key the user's sorts
    /// are remembered under (`remembered-list-sort`).
    pub(super) fn set_sorting(
        &mut self,
        default_sort: crate::ui::list_sort::Sort,
        remember_as: String,
    ) {
        self.default_sort = default_sort;
        self.remember_as = Some(remember_as);
    }

    /// What the row context menu's "Open" does with the row it was raised on.
    pub(super) fn set_on_open(&mut self, on_open: impl Fn(usize, &mut Window, &mut App) + 'static) {
        self.on_open = Some(std::rc::Rc::new(on_open));
    }

    /// What a modified or middle click on a row does with that row.
    pub(super) fn set_on_open_in_background(
        &mut self,
        open: impl Fn(&ListRow, &mut Window, &mut App) + 'static,
    ) {
        self.on_open_in_background = Some(std::rc::Rc::new(open));
    }

    /// Replaces the rows, keeping the active sort applied.
    pub(super) fn set_rows(&mut self, rows: Vec<ListRow>) {
        self.natural = rows;
        self.apply_sort();
    }

    /// The rows in their displayed order.
    pub(super) fn rows(&self) -> &[ListRow] {
        &self.rows
    }

    #[cfg(test)]
    pub(super) fn columns(&self) -> &[ListColumn] {
        &self.columns
    }

    /// The column layout to save: ids and widths, left to right.
    pub(super) fn layout(&self) -> ColumnLayout {
        self.columns
            .iter()
            .map(|column| SavedColumn {
                id: column.id.to_string(),
                width: f32::from(column.width),
            })
            .collect()
    }

    /// Records new widths for every column, left to right - the table's
    /// `ColumnWidthsChanged`.
    pub(super) fn set_widths(&mut self, widths: &[Pixels]) {
        for (column, width) in self.columns.iter_mut().zip(widths) {
            column.width = *width;
        }
    }

    /// Remembers which object the user selected, so a later sort or update can
    /// find it again.
    pub(super) fn remember_selection(&mut self, row_ix: usize) {
        self.selected = self.rows.get(row_ix).map(|row| {
            let (namespace, name) = row.key();
            (namespace.map(str::to_string), name.to_string())
        });
    }

    /// The displayed position of the remembered selection, if it is still listed.
    pub(super) fn selected_index(&self) -> Option<usize> {
        let (namespace, name) = self.selected.as_ref()?;
        self.rows
            .iter()
            .position(|row| row.key() == (namespace.as_deref(), name.as_str()))
    }

    /// Moves the column at `col_ix` to `to_ix`.
    pub(super) fn reorder_columns(&mut self, col_ix: usize, to_ix: usize) {
        let column = self.columns.remove(col_ix);
        self.columns.insert(to_ix, column);
    }

    /// Sorts by the column at `col_ix` in `sort`'s direction (already cycled by
    /// the table).
    #[cfg(test)]
    pub(super) fn resort(&mut self, col_ix: usize, sort: ColumnSort) {
        self.sort = Some((self.columns[col_ix].id.clone(), sort));
        self.apply_sort();
    }

    /// The active sort to save with the panel (`saved-panel-layouts` 1.6): its
    /// column's id and whether it's descending, or `None` for no active sort -
    /// `ColumnSort::Default` (the natural, unsorted order) counts as no sort
    /// here, the same as never having one.
    pub(super) fn sort_state(&self) -> Option<(&str, bool)> {
        match &self.sort {
            Some((id, ColumnSort::Ascending)) => Some((id.as_ref(), false)),
            Some((id, ColumnSort::Descending)) => Some((id.as_ref(), true)),
            Some((_, ColumnSort::Default)) | None => None,
        }
    }

    /// Applies a saved sort by column id (`saved-panel-layouts` 1.6): a
    /// `column_id` this build's columns don't have is ignored by
    /// [`Self::apply_sort`] the same way a reordered-away column already is -
    /// `rows` stays in its natural order.
    pub(super) fn set_sort(&mut self, column_id: &str, descending: bool) {
        let sort = if descending {
            ColumnSort::Descending
        } else {
            ColumnSort::Ascending
        };
        self.sort = Some((column_id.to_string().into(), sort));
        self.apply_sort();
    }

    /// Rebuilds `rows` from `natural`, never re-sorting `rows` in place, so
    /// `Default` restores the incoming order. Descending uses a reversed
    /// comparator, which keeps ties in their incoming order as a stable sort should.
    fn apply_sort(&mut self) {
        self.rows = self.natural.clone();
        let Some((id, sort)) = &self.sort else {
            return;
        };
        let Some(column) = self.columns.iter().find(|column| &column.id == id).cloned() else {
            return;
        };
        match sort {
            ColumnSort::Ascending => self.rows.sort_by(|a, b| compare(a, b, &column)),
            ColumnSort::Descending => self.rows.sort_by(|a, b| compare(b, a, &column)),
            ColumnSort::Default => {}
        }
    }
}

impl crate::ui::background_rows::BackgroundRows for ObjectTableDelegate {
    fn open_in_background(&mut self, row_ix: usize, window: &mut Window, cx: &mut App) {
        if let (Some(open), Some(row)) = (self.on_open_in_background.clone(), self.rows.get(row_ix))
        {
            open(row, window, cx);
        }
    }

    fn background_click(&mut self) -> &mut crate::ui::background_rows::BackgroundClick {
        &mut self.background_click
    }
}

impl TableDelegate for ObjectTableDelegate {
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
        let column = &self.columns[col_ix];
        let header = Column::new(column.id.clone(), column.title.clone())
            .width(column.width)
            .sortable();
        match &self.sort {
            Some((id, sort)) if id == &column.id => header.sort(*sort),
            _ => header,
        }
    }

    /// The mouse twin of Enter and double-click: open the row under the pointer.
    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(on_open) = self.on_open.clone() else {
            return menu;
        };
        menu.item(
            PopupMenuItem::new("Open")
                .on_click(move |_event, window, cx| on_open(row_ix, window, cx)),
        )
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        use crate::ui::style;
        use crate::ui::typography::TypeRole as _;
        let (row, column) = (&self.rows[row_ix], &self.columns[col_ix]);
        let cell = div()
            .debug_selector(|| format!("object-cell-{row_ix}-{col_ix}"))
            .data_font()
            .whitespace_nowrap();
        if column.id.as_ref() == FORWARDS {
            return cell.children(crate::ui::forward_indicator::indicator(
                &row.object.name,
                &row.forwards,
            ));
        }
        let text = cell_text(row, column);
        // A status cell's text takes its tone's colour; a readiness ratio
        // keeps plain text beside a dot in its tone, as the Pods table's Ready.
        let value = row.cell(column);
        match (value, value.and_then(columns::Cell::tone)) {
            (Some(columns::Cell::Readiness(..)), Some(tone)) => cell
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(style::status(tone, cx)),
                )
                .child(text),
            (_, Some(tone)) => cell.text_color(style::status(tone, cx)).child(text),
            (_, None) => cell.child(text),
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
        let title = self.columns[col_ix].title.clone();
        self.header.track(col_ix, div().size_full().child(title))
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _cx: &App) -> String {
        cell_text(&self.rows[row_ix], &self.columns[col_ix])
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        // The table's own next step is ignored: `list_sort` decides it.
        let _ = sort;
        crate::ui::list_sort::header_clicked(self, col_ix, window, cx);
    }
}

impl crate::ui::list_sort::SortableTable for ObjectTableDelegate {
    fn sort_columns(&self) -> Vec<SharedString> {
        self.columns
            .iter()
            .map(|column| column.id.clone())
            .collect()
    }

    fn current_sort(&self) -> Option<crate::ui::list_sort::Sort> {
        self.sort_state()
            .map(|(column, descending)| (SharedString::from(column.to_string()), descending))
    }

    fn sort_by(&mut self, (column, descending): &crate::ui::list_sort::Sort) {
        self.set_sort(column, *descending);
    }

    fn default_sort(&self) -> crate::ui::list_sort::Sort {
        self.default_sort.clone()
    }

    fn remember_as(&self) -> Option<&str> {
        self.remember_as.as_deref()
    }

    fn after_sort(table: &mut TableState<Self>, cx: &mut Context<TableState<Self>>) {
        reselect(table, cx);
    }
}

/// Re-points the table's row selection - which [`TableState`] keeps as a bare
/// index - at the object this table last selected, after a sort or a watch update
/// changed what sits at each index. Clears it if the object is gone.
pub(super) fn reselect(
    table: &mut TableState<ObjectTableDelegate>,
    cx: &mut Context<TableState<ObjectTableDelegate>>,
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

#[cfg(test)]
mod tests;

impl crate::ui::table_fit::FitColumns for ObjectTableDelegate {
    fn header_bounds(&self) -> &crate::ui::table_fit::HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get_mut(col_ix) {
            column.width = width;
        }
    }
}
