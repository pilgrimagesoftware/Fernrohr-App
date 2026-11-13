//! The Helm Releases table: its delegate (column order, widths, sort, the
//! displayed rows, the selection) and the `DataTable` it is drawn with.

use super::columns::{HelmColumn, status_tone};
use super::row::{Chart, HelmRow};
use crate::ui::table_fit::{FitColumns, HeaderBounds};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};
use gpui_kit::*;
use jiff::Timestamp;

/// The sort a list opens with: by name.
pub(crate) const DEFAULT_SORT: (HelmColumn, ColumnSort) = (HelmColumn::Name, ColumnSort::Ascending);

pub(crate) struct HelmTableDelegate {
    natural: Vec<HelmRow>,
    rows: Vec<HelmRow>,
    columns: Vec<HelmColumn>,
    sort: Option<(HelmColumn, ColumnSort)>,
    remember_as: Option<&'static str>,
    widths: Vec<(HelmColumn, Pixels)>,
    /// The release last selected, by uid, re-pointed after a sort or update.
    selected: Option<String>,
    now: Timestamp,
    header: HeaderBounds,
    no_match: bool,
    /// What a row's context menu needs: the panel's focus, whose key context
    /// its items' keys and actions resolve in, and why Uninstall can't run
    /// now, if it can't.
    menu: Option<(FocusHandle, Option<String>)>,
}

impl HelmTableDelegate {
    pub(crate) fn new(sort: Option<(HelmColumn, ColumnSort)>) -> Self {
        Self {
            natural: Vec::new(),
            rows: Vec::new(),
            columns: HelmColumn::DEFAULT_ORDER.to_vec(),
            sort,
            remember_as: None,
            widths: Vec::new(),
            selected: None,
            now: Timestamp::now(),
            header: HeaderBounds::default(),
            no_match: false,
            menu: None,
        }
    }

    pub(crate) fn remember_as(&mut self, key: &'static str) {
        self.remember_as = Some(key);
    }

    pub(crate) fn set_no_match(&mut self, no_match: bool) {
        self.no_match = no_match;
    }

    /// Gives rows their context menu: actions dispatched to `focus`, with
    /// Uninstall disabled and the reason shown while `unavailable`.
    pub(crate) fn set_menu(&mut self, focus: FocusHandle, unavailable: Option<String>) {
        self.menu = Some((focus, unavailable));
    }

    /// Replaces the rows, keeping the active sort applied.
    pub(crate) fn set_rows(&mut self, rows: Vec<HelmRow>, now: Timestamp) {
        self.natural = rows;
        self.now = now;
        self.apply_sort();
    }

    /// The rows in displayed order.
    #[cfg(test)]
    pub(crate) fn rows(&self) -> &[HelmRow] {
        &self.rows
    }

    pub(crate) fn sort(&self) -> Option<(HelmColumn, ColumnSort)> {
        self.sort
    }

    pub(crate) fn remember_selection(&mut self, row_ix: usize) {
        self.selected = self.rows.get(row_ix).map(|row| row.uid.clone());
    }

    /// The selected release, if it is still listed.
    pub(crate) fn selected(&self) -> Option<&HelmRow> {
        let uid = self.selected.as_deref()?;
        self.rows.iter().find(|row| row.uid == uid)
    }

    fn selected_index(&self) -> Option<usize> {
        let uid = self.selected.as_deref()?;
        self.rows.iter().position(|row| row.uid == uid)
    }

    fn width_of(&self, column: HelmColumn) -> Pixels {
        self.widths
            .iter()
            .find(|(col, _)| *col == column)
            .map_or(px(column.default_width()), |(_, width)| *width)
    }

    fn set_width(&mut self, column: HelmColumn, width: Pixels) {
        match self.widths.iter_mut().find(|(col, _)| *col == column) {
            Some((_, current)) => *current = width,
            None => self.widths.push((column, width)),
        }
    }

    /// The table's `ColumnWidthsChanged`: every column's width, left to right.
    pub(crate) fn set_widths(&mut self, widths: &[Pixels]) {
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

impl TableDelegate for HelmTableDelegate {
    /// A row's menu: Open, and Uninstall - disabled, with why beneath it,
    /// while `helm` or the connection isn't available. Opening it selects the
    /// row, which is what its items act on.
    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some((focus, unavailable)) = self.menu.clone() else {
            return menu;
        };
        if row_ix >= self.rows.len() {
            return menu;
        }
        cx.defer_in(window, move |table, _window, cx| {
            table.set_selected_row(row_ix, cx)
        });
        let menu = menu
            .action_context(focus)
            .menu("Open", Box::new(super::commands::OpenRelease))
            .separator()
            .menu_with_disabled(
                "Uninstall\u{2026}",
                Box::new(crate::k8s::resource::helm::actions::UninstallRelease),
                unavailable.is_some(),
            );
        match unavailable {
            Some(reason) => menu.label(reason),
            None => menu,
        }
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        crate::ui::list_state::empty_body(self.no_match, cx)
    }

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

    /// A status in its tone, with an icon for a failed or in-progress one so
    /// it reads without color; an unreadable chart in the bad tone; every cell
    /// in the data font.
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        use crate::ui::style::{self, Tone};
        use crate::ui::typography::TypeRole as _;
        use gpui_kit::assets::IconName;
        use gpui_kit::component::{Icon, Sizable as _};
        let row = &self.rows[row_ix];
        let column = self.columns[col_ix];
        let cell = div()
            .debug_selector(|| format!("helm-cell-{row_ix}-{col_ix}"))
            .data_font()
            .flex()
            .items_center()
            .gap_1()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis();
        let text = column.text(row, self.now);
        match column {
            HelmColumn::Status => {
                let tone = status_tone(&row.status);
                let icon = match tone {
                    Tone::Bad => Some(IconName::CircleAlert),
                    Tone::Warning => Some(IconName::RefreshCw),
                    _ => None,
                };
                cell.text_color(style::status(tone, cx))
                    .children(icon.map(|icon| Icon::new(icon).xsmall()))
                    .child(text)
            }
            HelmColumn::Chart if row.chart == Chart::Unreadable => {
                cell.text_color(style::status(Tone::Bad, cx)).child(text)
            }
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
        // The table's own next step is ignored: `list_sort` decides it.
        let _ = sort;
        crate::ui::list_sort::header_clicked(self, col_ix, window, cx);
    }
}

/// [`DEFAULT_SORT`] as `ui::list_sort` names a sort.
pub(crate) fn default_sort() -> crate::ui::list_sort::Sort {
    (SharedString::from(DEFAULT_SORT.0.id()), false)
}

impl crate::ui::list_sort::SortableTable for HelmTableDelegate {
    fn sort_columns(&self) -> Vec<SharedString> {
        self.columns
            .iter()
            .map(|column| SharedString::from(column.id()))
            .collect()
    }

    fn current_sort(&self) -> Option<crate::ui::list_sort::Sort> {
        match self.sort? {
            (column, ColumnSort::Ascending) => Some((column.id().into(), false)),
            (column, ColumnSort::Descending) => Some((column.id().into(), true)),
            (_, ColumnSort::Default) => None,
        }
    }

    fn sort_by(&mut self, (column, descending): &crate::ui::list_sort::Sort) {
        let Some(column) = HelmColumn::from_id(column) else {
            return;
        };
        let direction = if *descending {
            ColumnSort::Descending
        } else {
            ColumnSort::Ascending
        };
        self.sort = Some((column, direction));
        self.apply_sort();
    }

    fn default_sort(&self) -> crate::ui::list_sort::Sort {
        default_sort()
    }

    fn remember_as(&self) -> Option<&str> {
        self.remember_as
    }

    fn after_sort(table: &mut TableState<Self>, cx: &mut Context<TableState<Self>>) {
        reselect(table, cx);
    }
}

impl FitColumns for HelmTableDelegate {
    fn header_bounds(&self) -> &HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get(col_ix).copied() {
            self.set_width(column, width);
        }
    }
}

/// Re-points the selection at the release this table last selected, wherever
/// a sort or update moved it; clears it if the release is gone.
pub(crate) fn reselect(
    table: &mut TableState<HelmTableDelegate>,
    cx: &mut Context<TableState<HelmTableDelegate>>,
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

/// The table over `state`, drawn like the other list panels'.
pub(crate) fn data_table(
    state: &Entity<TableState<HelmTableDelegate>>,
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
