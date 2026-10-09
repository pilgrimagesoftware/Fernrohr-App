//! How a list table's sort moves (`remembered-list-sort`): the header click
//! cycle, the keyboard's sort commands, the sort a new table starts with, and
//! remembering the user's choice per kind.
//!
//! A table is always sorted. Clicking a header sorts by it ascending, then
//! descending, then returns to the table's default sort (design D4). On the
//! default column that third step is ascending again, so it alternates. The
//! keyboard does the same through three commands: Sort by Next Column and Sort
//! by Previous Column move the sort to a neighbouring column, ascending (a
//! first click on that header), and Cycle Sort takes the sorted column one
//! step round the cycle (another click on it).
//!
//! Only those two routes are the user's, so only they record the sort for the
//! table's kind (D2). Starting a table - from its own saved sort, the
//! remembered one, or the default - records nothing.
//!
//! gpui-component's table cycles a header Default, Descending, Ascending on
//! its own, and keeps the header's arrow in state the delegate can only reset
//! by refreshing the table. So a click hands [`header_clicked`] the column,
//! not the table's idea of the next step, and the table is refreshed after.

use crate::command::{Command, CommandRegistry};
use crate::config::workspace::SortState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use gpui_kit::component::table::{TableDelegate, TableState};
use gpui_kit::*;

actions!(list_sort, [SortNextColumn, SortPreviousColumn, CycleSort]);

/// The key context every sortable list panel adds, so one set of sort
/// commands serves them all.
pub(crate) const KEY_CONTEXT: &str = "SortableList";
const COMMAND_CONTEXT: &str = "SortableList && !Input";

const NEXT_KEY: &str = "shift-.";
const PREVIOUS_KEY: &str = "shift-,";
const CYCLE_KEY: &str = "o";

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, title, default_binding, action) in [
        (
            "list.sort_next_column",
            "Sort by Next Column",
            NEXT_KEY,
            Box::new(SortNextColumn) as Box<dyn Action>,
        ),
        (
            "list.sort_previous_column",
            "Sort by Previous Column",
            PREVIOUS_KEY,
            Box::new(SortPreviousColumn),
        ),
        (
            "list.cycle_sort",
            "Cycle Sort",
            CYCLE_KEY,
            Box::new(CycleSort),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(COMMAND_CONTEXT),
            action,
            menu: None,
        });
    }
}

/// A sort: a column id, and whether it is descending.
pub(crate) type Sort = (SharedString, bool);

/// A list table's delegate, as far as its sort goes. Columns are named by id
/// throughout, the same ids a saved and a remembered sort store.
pub(crate) trait SortableTable: TableDelegate + Sized {
    /// The column ids, in their displayed order.
    fn sort_columns(&self) -> Vec<SharedString>;
    /// The active sort.
    fn current_sort(&self) -> Option<Sort>;
    /// Sorts by `sort`, a column this table has.
    fn sort_by(&mut self, sort: &Sort);
    /// The sort Cycle Sort and a third click return to.
    fn default_sort(&self) -> Sort;
    /// The [`SortDefaults`](crate::util::shell::SortDefaults) key a user's sort
    /// is remembered under; `None` remembers nothing.
    fn remember_as(&self) -> Option<&str>;
    /// Called after the sort changed, to re-point the selection at the row
    /// that moved.
    fn after_sort(table: &mut TableState<Self>, cx: &mut Context<TableState<Self>>);
}

/// The sort a click on `column`'s header moves `current` to.
pub(crate) fn clicked(current: Option<&Sort>, column: &SharedString, default: &Sort) -> Sort {
    match current {
        Some((sorted, false)) if sorted == column => (column.clone(), true),
        Some((sorted, true)) if sorted == column && column != &default.0 => default.clone(),
        _ => (column.clone(), false),
    }
}

/// A keyboard sort step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Next,
    Previous,
    Cycle,
}

/// The sort `step` moves `current` to, over `columns` in displayed order.
pub(crate) fn stepped(
    current: Option<&Sort>,
    columns: &[SharedString],
    default: &Sort,
    step: Step,
) -> Sort {
    let sorted = current.map_or(&default.0, |(column, _)| column);
    let position = columns.iter().position(|column| column == sorted);
    let neighbour = |offset: usize| {
        let column = match position {
            Some(ix) => &columns[(ix + offset) % columns.len()],
            None => &default.0,
        };
        (column.clone(), false)
    };
    match step {
        _ if columns.is_empty() => default.clone(),
        Step::Next => neighbour(1),
        Step::Previous => neighbour(columns.len() - 1),
        Step::Cycle => clicked(current, sorted, default),
    }
}

/// The sort a table starts with (design D3): its own saved one, else the
/// remembered one for its kind, else `default`. A saved or remembered sort
/// whose column `columns` no longer has is passed over, so the table is still
/// sorted.
pub(crate) fn starting(
    own: Option<Sort>,
    remembered: Option<SortState>,
    columns: &[SharedString],
    default: Sort,
) -> Sort {
    let exists = |sort: &Sort| columns.contains(&sort.0);
    let remembered = remembered.map(|sort| (SharedString::from(sort.column), !sort.ascending));
    own.filter(exists)
        .or(remembered.filter(exists))
        .unwrap_or(default)
}

/// The key a list kind's sort is remembered under: its API group and kind
/// (`apps/Deployment`, `/Pod` for the core group), which stay the same from
/// one cluster's discovery to the next.
pub(crate) fn kind_key(kind: &DiscoveredKind) -> String {
    format!("{}/{}", kind.gvk.group, kind.gvk.kind)
}

/// The events browser's key: it is a list of its own, not the Event kind's.
pub(crate) const EVENTS_KEY: &str = "events";

/// A header click on `col_ix`, from the delegate's `perform_sort`.
pub(crate) fn header_clicked<D: SortableTable + 'static>(
    delegate: &mut D,
    col_ix: usize,
    window: &mut Window,
    cx: &mut Context<TableState<D>>,
) {
    let Some(column) = delegate.sort_columns().get(col_ix).cloned() else {
        return;
    };
    let next = clicked(
        delegate.current_sort().as_ref(),
        &column,
        &delegate.default_sort(),
    );
    choose(delegate, &next, cx);
    // Mid-update on the table here: its header arrows are rebuilt once it is
    // free again.
    cx.defer_in(window, |table, _window, cx| {
        table.refresh(cx);
        D::after_sort(table, cx);
    });
}

/// A keyboard sort `step` on `table`.
pub(crate) fn step<D: SortableTable + 'static>(
    table: &Entity<TableState<D>>,
    step: Step,
    cx: &mut App,
) {
    table.update(cx, |table, cx| {
        let delegate = table.delegate();
        let next = stepped(
            delegate.current_sort().as_ref(),
            &delegate.sort_columns(),
            &delegate.default_sort(),
            step,
        );
        choose(table.delegate_mut(), &next, cx);
        table.refresh(cx);
        D::after_sort(table, cx);
        cx.notify();
    });
}

/// `element` handling the three sort commands for the table `table` reads off
/// its view: what each sortable list panel puts on its root, beside
/// [`KEY_CONTEXT`].
pub(crate) fn on_sort_actions<V, D, E>(
    element: E,
    table: fn(&V) -> Option<Entity<TableState<D>>>,
    cx: &mut Context<V>,
) -> E
where
    V: 'static,
    D: SortableTable + 'static,
    E: InteractiveElement,
{
    let run = move |view: &V, step: Step, cx: &mut App| {
        if let Some(table) = table(view) {
            self::step(&table, step, cx);
        }
    };
    element
        .on_action(cx.listener(move |view, _: &SortNextColumn, _, cx| run(view, Step::Next, cx)))
        .on_action(
            cx.listener(move |view, _: &SortPreviousColumn, _, cx| run(view, Step::Previous, cx)),
        )
        .on_action(cx.listener(move |view, _: &CycleSort, _, cx| run(view, Step::Cycle, cx)))
}

/// Applies a sort the user chose, and remembers it for the table's kind.
fn choose<D: SortableTable>(delegate: &mut D, sort: &Sort, cx: &mut App) {
    delegate.sort_by(sort);
    if let Some(key) = delegate.remember_as() {
        let (column, descending) = sort;
        crate::util::shell::SortDefaults::record(
            cx,
            key,
            SortState {
                column: column.to_string(),
                ascending: !descending,
            },
        );
    }
}

#[cfg(test)]
mod tests;
