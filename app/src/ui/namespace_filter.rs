//! A filterable, multi-select namespace list (`namespace-picker-filter`): a
//! filter input over "All namespaces", the saved namespace groups (#311), and
//! the cluster's namespaces, each row checked when picked.
//!
//! Typing narrows the groups and namespaces to those whose name contains the
//! text, ignoring case; "All namespaces" stays listed first whatever the
//! filter says. A group's row scopes to all of its namespaces at once - the
//! ones the cluster still has, and any it doesn't, which match nothing - and
//! is checked while the selection is exactly that group's. Up and Down
//! move the highlight, Enter or a click toggles the highlighted entry, and the
//! first Escape clears the filter text - the second goes on to whatever hosts
//! the list, which closes. A filter matching no namespace says so.
//!
//! Built on gpui-kit's `Command`, the widget the cluster picker and the
//! command palette use, with its own filtering off so the pinned entry
//! survives the query. It is drawn from data the host passes in on each render
//! and holds no selection of its own: the host owns the namespaces picked, and
//! hears each toggle as the selection that results ([`OnPick`]). That keeps it
//! usable both in the title bar's picker and inline in another editor.

use crate::config::namespaces::NamespaceSetConfig;
use crate::ui::namespace_sets::store::NamespaceSets;
use crate::ui::panel_title::namespaces_offered;
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// The pinned entry's label: picking it clears the selection.
pub const ALL_NAMESPACES: &str = "All namespaces";

/// What the list says when the filter matches no namespace.
pub const NO_MATCH: &str = "No matching namespaces";

/// What a toggle reports: the selection it leads to.
pub type OnPick = Rc<dyn Fn(Vec<String>, &mut Window, &mut App)>;

/// What a filter change runs: the host draws the list again for the new text.
pub type OnQuery = Rc<dyn Fn(&mut Window, &mut App)>;

/// The entries listed for `query`: "All namespaces" (`None`), then each of
/// `namespaces` containing `query`, ignoring case, in their given order.
pub fn entries(namespaces: &[String], query: &str) -> Vec<Option<String>> {
    let query = query.trim().to_lowercase();
    namespaces_offered(namespaces)
        .into_iter()
        .filter(|entry| match entry {
            None => true,
            Some(name) => name.to_lowercase().contains(&query),
        })
        .collect()
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    /// "All namespaces": picking it clears the selection.
    All,
    /// A saved namespace group, picked whole.
    Group(NamespaceSetConfig),
    Namespace(String),
}

/// The saved groups whose name contains `query`, ignoring case, in their
/// saved order.
pub fn groups_matching<'a>(
    groups: &'a [NamespaceSetConfig],
    query: &str,
) -> impl Iterator<Item = &'a NamespaceSetConfig> {
    let query = query.trim().to_lowercase();
    groups
        .iter()
        .filter(move |group| group.name.to_lowercase().contains(&query))
}

/// A group row's badge: how many namespaces it holds - and, once the
/// cluster's namespaces are known, how many of them it still has, when that's
/// fewer.
pub fn member_count(members: &[String], cluster: &[String]) -> String {
    let total = members.len();
    let present = members
        .iter()
        .filter(|member| cluster.contains(member))
        .count();
    let noun = if total == 1 {
        "namespace"
    } else {
        "namespaces"
    };
    if cluster.is_empty() || present == total {
        format!("{total} {noun}")
    } else {
        format!("{present} of {total} {noun}")
    }
}

/// Whether `selected` is exactly `group`'s namespaces, in any order.
fn is_group(selected: &[String], group: &NamespaceSetConfig) -> bool {
    let mut members = group.namespaces.clone();
    members.sort_unstable();
    members.dedup();
    let mut selected = selected.to_vec();
    selected.sort_unstable();
    selected.dedup();
    !members.is_empty() && members == selected
}

/// The selection after toggling `entry` in `selected`: "All namespaces"
/// clears it, a namespace is added (kept sorted) or removed.
pub fn toggled(selected: &[String], entry: Option<&str>) -> Vec<String> {
    let Some(namespace) = entry else {
        return Vec::new();
    };
    if selected.iter().any(|picked| picked == namespace) {
        return selected
            .iter()
            .filter(|picked| *picked != namespace)
            .cloned()
            .collect();
    }
    let mut next = selected.to_vec();
    next.push(namespace.to_string());
    next.sort_unstable();
    next.dedup();
    next
}

/// The debug selector of `entry`'s row label.
pub fn option_selector(entry: Option<&str>) -> String {
    format!("namespace-option {}", entry.unwrap_or(ALL_NAMESPACES))
}

/// The debug selector `entry`'s row label carries while it is checked.
pub fn checked_selector(entry: Option<&str>) -> String {
    format!("namespace-checked {}", entry.unwrap_or(ALL_NAMESPACES))
}

/// The debug selector of the "not in this cluster" mark on `namespace`'s row.
pub fn absent_selector(namespace: &str) -> String {
    format!("namespace-absent {namespace}")
}

/// The debug selector of group `name`'s row label.
pub fn group_selector(name: &str) -> String {
    format!("namespace-group {name}")
}

/// The debug selector group `name`'s row label carries while it is checked.
pub fn group_checked_selector(name: &str) -> String {
    format!("namespace-group-checked {name}")
}

/// The debug selector of group `name`'s member count.
pub fn group_count_selector(name: &str) -> String {
    format!("namespace-group-count {name}")
}

/// The mark beside a listed namespace the connected cluster doesn't have.
pub const ABSENT: &str = "not in this cluster";

/// The debug selector of the no-match message.
pub const NO_MATCH_SELECTOR: &str = "namespace-filter-empty";

/// The list's own state: its filter text and highlight, and the row the
/// keyboard chose. Cheap to clone - a handle on one shared state.
///
/// `Command` moves its highlight under the pointer too, and confirms the
/// highlight on Enter; hover must never move the selection, so Enter acts on
/// the row the keyboard last chose (`selected`) rather than on the highlight.
#[derive(Clone)]
pub struct NamespaceFilter {
    command: Entity<CommandState>,
    selected: Rc<Cell<Option<usize>>>,
    /// Whether "All namespaces" is listed first, then the saved groups - the
    /// picker's ways to clear a scope or set a group's at once. A set editor
    /// picks a group's members one by one, so it leaves both out.
    pin_all: bool,
}

impl NamespaceFilter {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            command: cx.new(|cx| CommandState::new(window, cx)),
            selected: Rc::new(Cell::new(Some(0))),
            pin_all: true,
        }
    }

    /// The list without the pinned "All namespaces" entry: only namespaces.
    pub fn without_all(window: &mut Window, cx: &mut App) -> Self {
        Self {
            pin_all: false,
            ..Self::new(window, cx)
        }
    }

    /// The filter text.
    pub fn query(&self, cx: &App) -> SharedString {
        self.command.read(cx).query(cx)
    }

    /// Empties the filter, listing every namespace again, with the first
    /// entry chosen.
    pub fn reset(&self, window: &mut Window, cx: &mut App) {
        self.command
            .update(cx, |command, cx| command.set_query("", window, cx));
        self.selected.set(Some(0));
    }

    /// The filter input's focus handle: what a host focuses to type into it.
    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command.read(cx).focus_handle(cx)
    }

    /// Draws the list over `namespaces` with `selected` checked. A toggle
    /// reports the selection it leads to through `on_pick`; a filter change
    /// calls `on_query`, so the host draws the list again.
    pub fn element(
        &self,
        namespaces: &[String],
        selected: &[String],
        on_pick: OnPick,
        on_query: OnQuery,
        cx: &App,
    ) -> AnyElement {
        self.element_marked(namespaces, selected, &[], on_pick, on_query, cx)
    }

    /// [`Self::element`], with each of `absent` - listed namespaces the
    /// connected cluster doesn't have - marked as such.
    pub fn element_marked(
        &self,
        namespaces: &[String],
        selected: &[String],
        absent: &[String],
        on_pick: OnPick,
        on_query: OnQuery,
        cx: &App,
    ) -> AnyElement {
        let query = self.query(cx);
        let groups: Vec<Entry> = if self.pin_all {
            groups_matching(&NamespaceSets::get(cx).sets, &query)
                .cloned()
                .map(Entry::Group)
                .collect()
        } else {
            Vec::new()
        };
        let mut listed: Vec<Entry> = Vec::new();
        for entry in entries(namespaces, &query) {
            match entry {
                None if self.pin_all => {
                    listed.push(Entry::All);
                    listed.extend(groups.iter().cloned());
                }
                None => {}
                Some(namespace) => listed.push(Entry::Namespace(namespace)),
            }
        }
        let pinned = usize::from(self.pin_all);
        let no_match = listed.len() == pinned && !query.trim().is_empty();
        let muted = cx.theme().muted_foreground;
        let items = listed.iter().map(|entry| match entry {
            Entry::Group(group) => group_item(group, namespaces, selected, muted),
            Entry::All | Entry::Namespace(_) => {
                let entry = match entry {
                    Entry::Namespace(namespace) => Some(namespace.clone()),
                    _ => None,
                };
                namespace_item(entry, selected, absent, muted)
            }
        });
        let selected = selected.to_vec();
        let keyboard_row = self.selected.clone();
        let confirmed_row = self.selected.clone();
        let command = Command::new(&self.command)
            .items(items)
            .filterable(false)
            .bordered(false)
            .placeholder("Filter namespaces…")
            .on_query(move |_query, window, cx| on_query(window, cx))
            // Arrows, and typing (which moves the highlight back to the top),
            // choose a row; hover moves only `Command`'s own highlight.
            .on_select(move |index_path, window, _cx| {
                if window.last_input_was_keyboard() {
                    keyboard_row.set(Some(index_path.row));
                }
            })
            // Rows are positions in `listed`: with filtering off, `Command`
            // reports the index among the items it was given. Enter toggles
            // the keyboard's row; a click toggles the row clicked.
            .on_confirm(move |index_path, window, cx| {
                let row = if window.last_input_was_keyboard() {
                    confirmed_row.get().unwrap_or(index_path.row)
                } else {
                    index_path.row
                };
                // A click chooses its row, as the arrows do.
                confirmed_row.set(Some(row));
                let next = match listed.get(row) {
                    Some(Entry::All) => toggled(&selected, None),
                    Some(Entry::Namespace(namespace)) => toggled(&selected, Some(namespace)),
                    Some(Entry::Group(group)) => group.namespaces.clone(),
                    None => return,
                };
                on_pick(next, window, cx);
            });
        div()
            .flex()
            .flex_col()
            .child(command)
            .when(no_match, |this| {
                this.child(
                    div()
                        .debug_selector(|| NO_MATCH_SELECTOR.into())
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(muted)
                        .child(NO_MATCH),
                )
            })
            .into_any_element()
    }
}

/// The row for "All namespaces" (`entry` `None`) or a namespace: checked
/// while picked, and marked when the cluster doesn't have it.
fn namespace_item(
    entry: Option<String>,
    selected: &[String],
    absent: &[String],
    muted: Hsla,
) -> CommandItem {
    let checked = match &entry {
        None => selected.is_empty(),
        Some(namespace) => selected.contains(namespace),
    };
    let label = entry.clone().unwrap_or_else(|| ALL_NAMESPACES.to_string());
    let selector = option_selector(entry.as_deref());
    let checked_selector = checked.then(|| checked_selector(entry.as_deref()));
    let absent = entry
        .as_ref()
        .filter(|namespace| absent.contains(namespace))
        .map(|namespace| absent_selector(namespace));
    let text = label.clone();
    CommandItem::new()
        .label(label)
        .checked(checked)
        .child(move |_window, _cx| {
            let selector = selector.clone();
            div()
                .flex_1()
                .flex()
                .gap_2()
                .debug_selector(move || selector)
                .child(
                    div()
                        .when_some(checked_selector.clone(), |this, selector| {
                            this.debug_selector(move || selector)
                        })
                        .child(text.clone()),
                )
                .when_some(absent.clone(), |this, selector| {
                    this.child(
                        div()
                            .debug_selector(move || selector)
                            .text_color(muted)
                            .child(ABSENT),
                    )
                })
        })
}

/// A saved group's row: a group icon, its name, and how many namespaces it
/// holds; checked while the selection is exactly its namespaces.
fn group_item(
    group: &NamespaceSetConfig,
    cluster: &[String],
    selected: &[String],
    muted: Hsla,
) -> CommandItem {
    let checked = is_group(selected, group);
    let name = group.name.clone();
    let count = member_count(&group.namespaces, cluster);
    CommandItem::new()
        .label(name.clone())
        .checked(checked)
        .child(move |_window, _cx| {
            let row = group_selector(&name);
            let checked_selector = checked.then(|| group_checked_selector(&name));
            let count_selector = group_count_selector(&name);
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap_2()
                .debug_selector(move || row)
                .child(
                    gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Folder)
                        .xsmall()
                        .text_color(muted),
                )
                .child(
                    div()
                        .when_some(checked_selector, |this, selector| {
                            this.debug_selector(move || selector)
                        })
                        .child(name.clone()),
                )
                .child(
                    div()
                        .debug_selector(move || count_selector)
                        .text_xs()
                        .text_color(muted)
                        .child(count.clone()),
                )
        })
}

#[cfg(test)]
mod tests;
