//! Up/Down on a focused list panel (`standard-resource-panels` 5.2): with no row
//! selected, Down selects the first visible row and Up the last, and either works
//! while focus is on the panel rather than its table.
//!
//! gpui-component's table binds Up/Down, and handles them, only in its own
//! `DataTable` element, so a list panel focused as a whole - newly opened, or its
//! tab picked, which focuses the panel rather than its table - had no row to move
//! to. Each list panel binds the table's own `SelectDown`/`SelectUp` in its key
//! context too ([`bindings`]) and catches them in its capture phase with [`step`].
//! A table that already has focus and a selection is left to step itself.

use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::table::{TableDelegate, TableState};
use gpui_kit::*;

/// Which way the cursor moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Down,
    Up,
}

/// Up and Down in each of `contexts` - list panels' key contexts. Bound raw, not
/// as commands: pure cursor movement is the keyboard rule's one exception.
pub fn bindings(contexts: &[&'static str]) -> Vec<KeyBinding> {
    contexts
        .iter()
        .flat_map(|&context| {
            [
                KeyBinding::new("down", SelectDown, Some(context)),
                KeyBinding::new("up", SelectUp, Some(context)),
            ]
        })
        .collect()
}

/// The row a step lands on among `rows` visible rows, from `selected`.
fn target_row(rows: usize, selected: Option<usize>, step: Step) -> Option<usize> {
    let last = rows.checked_sub(1)?;
    Some(match (selected, step) {
        (None, Step::Down) => 0,
        (None, Step::Up) => last,
        (Some(row), Step::Down) => (row + 1).min(last),
        (Some(row), Step::Up) => row.saturating_sub(1),
    })
}

/// Moves `table`'s selection one `step` and focuses the table, when focus is on
/// `panel` itself or on the table with nothing selected. A table with focus and
/// a selection steps itself; anything else focused inside the panel - a row's
/// context menu, a picker - keeps Up/Down for its own list. Returns whether it
/// moved, so the caller stops the action going further.
pub fn step<D: TableDelegate>(
    table: &Entity<TableState<D>>,
    panel: &FocusHandle,
    step: Step,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let state = table.read(cx);
    let selected = state.selected_row();
    let table_focused = state.focus_handle(cx).is_focused(window);
    if (table_focused && selected.is_some()) || (!table_focused && !panel.is_focused(window)) {
        return false;
    }
    let Some(row) = target_row(state.delegate().rows_count(cx), selected, step) else {
        return false;
    };
    table.update(cx, |table, cx| table.set_selected_row(row, cx));
    let focus = table.read(cx).focus_handle(cx);
    window.focus(&focus, cx);
    true
}

#[cfg(test)]
mod tests {
    use super::{Step, target_row};

    #[test]
    fn from_no_selection_down_is_the_first_row_and_up_the_last() {
        assert_eq!(target_row(5, None, Step::Down), Some(0));
        assert_eq!(target_row(5, None, Step::Up), Some(4));
        assert_eq!(target_row(5, Some(2), Step::Down), Some(3));
        assert_eq!(target_row(5, Some(4), Step::Down), Some(4));
        assert_eq!(target_row(5, Some(0), Step::Up), Some(0));
        assert_eq!(target_row(0, None, Step::Down), None);
    }
}
