//! A list panel's table: which columns it has, how two rows compare on one, and
//! the [`gpui_kit`] table delegate that renders, sorts and reorders them.
//!
//! Columns are identified by a string key, not a position or a closed enum
//! (`standard-resource-panels` D2), so a saved column order and widths survive a
//! kind gaining columns, and the per-kind columns join the base ones without a
//! new type per kind. Every kind has the base columns - Name, Namespace for a
//! namespaced kind, and Age - and a built-in kind its own between them
//! ([`super::columns`]).

use std::cmp::Ordering;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};
use gpui_kit::*;
use serde::{Deserialize, Serialize};

use super::columns;
use super::row::ObjectRow;
use crate::k8s::cluster::discovery::DiscoveredKind;

/// The base columns' keys. Also what [`ColumnLayout`] saves.
pub(super) const NAME: &str = "name";
pub(super) const NAMESPACE: &str = "namespace";
pub(super) const AGE: &str = "age";

/// One column of a list table.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ListColumn {
    pub(super) id: SharedString,
    title: SharedString,
    pub(super) width: Pixels,
    /// For one of the kind's own columns, which of a row's
    /// [`ObjectRow::cells`] it shows; `None` for a base column.
    cell: Option<usize>,
}

impl ListColumn {
    fn new(id: &'static str, title: &'static str, width: f32) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: px(width),
            cell: None,
        }
    }

    /// `kind`'s columns in their default order, as `kubectl get` lays them
    /// out: Name, Namespace for a namespaced kind only, the kind's own columns,
    /// then Age.
    pub(super) fn for_kind(kind: &DiscoveredKind) -> Vec<ListColumn> {
        let mut columns = vec![ListColumn::new(NAME, "Name", 260.)];
        if kind.namespaced {
            columns.push(ListColumn::new(NAMESPACE, "Namespace", 150.));
        }
        if let Some(own) = columns::for_kind(&kind.gvk.group, &kind.gvk.kind) {
            columns.extend(
                own.columns
                    .iter()
                    .enumerate()
                    .map(|(cell, def)| ListColumn {
                        cell: Some(cell),
                        ..ListColumn::new(def.id, def.title, def.width)
                    }),
            );
        }
        columns.push(ListColumn::new(AGE, "Age", 70.));
        columns
    }
}

/// How one column is laid out: what a panel saves so its columns come back in the
/// order and at the widths the user left them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct SavedColumn {
    pub(super) id: String,
    pub(super) width: f32,
}

/// A table's column layout, left to right.
pub(super) type ColumnLayout = Vec<SavedColumn>;

/// `columns` rearranged and resized to `saved`: the saved columns first, in their
/// saved order and widths, then any column `saved` doesn't mention in its default
/// place. A saved column the kind no longer has is dropped.
pub(super) fn apply_layout(columns: Vec<ListColumn>, saved: &[SavedColumn]) -> Vec<ListColumn> {
    let mut remaining = columns;
    let mut ordered = Vec::with_capacity(remaining.len());
    for saved in saved {
        if let Some(position) = remaining.iter().position(|column| column.id == saved.id) {
            let mut column = remaining.remove(position);
            column.width = px(saved.width);
            ordered.push(column);
        }
    }
    ordered.extend(remaining);
    ordered
}

/// One listed object as the table shows it: the row, its age now, and the
/// moment that "now" was - taken once per refresh, so every time-like cell's
/// sort and text agree.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ListRow {
    pub(super) object: ObjectRow,
    pub(super) age_secs: i64,
    now: jiff::Timestamp,
}

impl ListRow {
    pub(super) fn new(object: ObjectRow, now: jiff::Timestamp) -> Self {
        let age_secs = object.age_secs(now);
        Self {
            object,
            age_secs,
            now,
        }
    }

    /// The row's cell for `column`, if it is one of the kind's own columns.
    fn cell(&self, column: &ListColumn) -> Option<&columns::Cell> {
        self.object.cells.get(column.cell?)
    }

    /// The object's identity within its kind: namespace (if any) and name. What
    /// selection follows across sorts and watch updates.
    pub(super) fn key(&self) -> (Option<&str>, &str) {
        (self.object.namespace.as_deref(), &self.object.name)
    }
}

/// How `a` and `b` order on `column`. Age sorts by seconds, and a kind's own
/// column by its cells' values - numerically where they are numbers.
pub(super) fn compare(a: &ListRow, b: &ListRow, column: &ListColumn) -> Ordering {
    if column.cell.is_some() {
        let empty = columns::Cell::Empty;
        let (a_cell, b_cell) = (
            a.cell(column).unwrap_or(&empty),
            b.cell(column).unwrap_or(&empty),
        );
        return a_cell.compare(b_cell, a.now);
    }
    match column.id.as_ref() {
        NAME => a.object.name.cmp(&b.object.name),
        NAMESPACE => a.object.namespace.cmp(&b.object.namespace),
        AGE => a.age_secs.cmp(&b.age_secs),
        // Every column `ListColumn::for_kind` makes is handled above; a key from
        // elsewhere (a hand-edited layout) has nothing to compare by.
        _ => Ordering::Equal,
    }
}

/// `column`'s text for `row`.
pub(super) fn cell_text(row: &ListRow, column: &ListColumn) -> String {
    if column.cell.is_some() {
        return row
            .cell(column)
            .map(|cell| cell.display(row.now))
            .unwrap_or_default();
    }
    match column.id.as_ref() {
        NAME => row.object.name.clone(),
        NAMESPACE => row.object.namespace.clone().unwrap_or_default(),
        AGE => crate::k8s::resource::pods::format_age(row.age_secs),
        _ => String::new(),
    }
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
        Self {
            natural: Vec::new(),
            rows: Vec::new(),
            columns,
            sort: None,
            selected: None,
            on_open: None,
            on_open_in_background: None,
            background_click: Default::default(),
            header: Default::default(),
        }
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
    pub(super) fn resort(&mut self, col_ix: usize, sort: ColumnSort) {
        self.sort = Some((self.columns[col_ix].id.clone(), sort));
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
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        use crate::ui::typography::TypeRole as _;
        div()
            .debug_selector(|| format!("object-cell-{row_ix}-{col_ix}"))
            .data_font()
            .whitespace_nowrap()
            .child(cell_text(&self.rows[row_ix], &self.columns[col_ix]))
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
        self.resort(col_ix, sort);
        // Mid-update on the table here, so re-pointing the selection waits for the
        // end of this cycle, when the entity is free again.
        cx.defer_in(window, |table, _window, cx| reselect(table, cx));
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
