//! The rows a list panel has checked - by identity, so a sort or a watch update
//! that moves a row keeps it checked - and the checkbox drawn in each row's
//! first cell.
//!
//! Checked is beside the table's single cursor, not instead of it: checking a
//! row never moves the cursor, and the single-row commands keep acting on the
//! cursor's row.

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::*;
use std::collections::HashSet;

/// A row's object: namespace (none for a cluster-scoped kind) and name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RowKey {
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
}

impl RowKey {
    pub(crate) fn new(namespace: Option<&str>, name: &str) -> Self {
        Self {
            namespace: namespace.map(str::to_string),
            name: name.to_string(),
        }
    }
}

/// A table's checked rows, and the last row checked or unchecked one at a
/// time - where a range check starts.
#[derive(Clone, Debug, Default)]
pub(crate) struct CheckedRows {
    checked: HashSet<RowKey>,
    anchor: Option<RowKey>,
}

impl CheckedRows {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.checked.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.checked.len()
    }

    pub(crate) fn contains(&self, key: &RowKey) -> bool {
        self.checked.contains(key)
    }

    /// Checks `key`, or unchecks it, and makes it where the next range starts.
    pub(crate) fn toggle(&mut self, key: RowKey) {
        if !self.checked.remove(&key) {
            self.checked.insert(key.clone());
        }
        self.anchor = Some(key);
    }

    /// Checks every row from the range's start to `target`, both included, in
    /// `rows`' displayed order. With no start still listed, checks `target`
    /// alone. `target` becomes the next range's start.
    pub(crate) fn check_range(&mut self, rows: &[RowKey], target: &RowKey) {
        let to = rows.iter().position(|row| row == target);
        let from = self
            .anchor
            .as_ref()
            .and_then(|anchor| rows.iter().position(|row| row == anchor));
        match (from, to) {
            (Some(from), Some(to)) => {
                let (start, end) = (from.min(to), from.max(to));
                self.checked.extend(rows[start..=end].iter().cloned());
            }
            _ => {
                self.checked.insert(target.clone());
            }
        }
        self.anchor = Some(target.clone());
    }

    /// Checks every row in `rows`, keeping where the next range starts.
    pub(crate) fn check_all(&mut self, rows: &[RowKey]) {
        self.checked.extend(rows.iter().cloned());
    }

    /// Drops every checked row `listed` no longer has - a deleted object's.
    pub(crate) fn retain_listed(&mut self, listed: &[RowKey]) {
        let listed: HashSet<&RowKey> = listed.iter().collect();
        self.checked.retain(|key| listed.contains(key));
        if self
            .anchor
            .as_ref()
            .is_some_and(|key| !listed.contains(key))
        {
            self.anchor = None;
        }
    }

    /// Unchecks `keys`, leaving the rest checked.
    pub(crate) fn uncheck(&mut self, keys: &[RowKey]) {
        for key in keys {
            self.checked.remove(key);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.checked.clear();
        self.anchor = None;
    }

    /// The checked rows in `rows`' displayed order.
    #[cfg(test)]
    pub(crate) fn in_order<'a>(&self, rows: &'a [RowKey]) -> Vec<&'a RowKey> {
        rows.iter().filter(|row| self.contains(row)).collect()
    }
}

/// `cell` with row `row_ix`'s checkbox before it. A click on the checkbox calls
/// `on_check` with whether Shift was held, and goes no further: the row under
/// it neither selects nor opens.
///
/// The checkbox is no tab stop - a table has one per row - since `x` and
/// Shift+`x` check rows from the keyboard.
pub(crate) fn with_checkbox(
    id_prefix: &str,
    row_ix: usize,
    checked: bool,
    cell: impl IntoElement,
    on_check: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Div {
    let id = format!("{id_prefix}-check-{row_ix}");
    let checkbox = div()
        .id(SharedString::from(id.clone()))
        .debug_selector({
            let id = id.clone();
            move || id.clone()
        })
        .flex_shrink_0()
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_check(event.modifiers().shift, window, cx);
        })
        .child(
            Checkbox::new(SharedString::from(format!("{id}-box")))
                .checked(checked)
                .tab_stop(false),
        );
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(checkbox)
        .child(cell)
}

#[cfg(test)]
mod tests;
