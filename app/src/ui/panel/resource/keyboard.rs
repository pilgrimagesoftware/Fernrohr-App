//! Keyboard machinery for the Resource panel: `.claude/rules/keyboard-first.md`
//! requires that the grouped, filterable list section 1-3 add stay fully
//! keyboard-operable, with one selection that clicks and keys both move.
//!
//! This module owns the panel's own actions and key bindings, the pure
//! visible-row/movement logic Up/Down/Left/Right read (GPUI-free, so it is
//! directly testable), and the hint row that names the bound keys. `/` is not
//! here: it is a [`crate::command::Command`] registered in `resource.rs`
//! itself, so `keymap::bindings` picks it up automatically (see that file's
//! `register_commands`).

use super::section::{Navigable, VisibleSection};
use crate::ui::nav::NavTarget;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

actions!(
    resource_panel,
    [
        SelectNext,
        SelectPrevious,
        OpenSelected,
        CollapseSection,
        ExpandSection,
        ToggleSubgroup
    ]
);

/// The `KeyContext` the panel's own bindings (and `/`'s `Command`) are scoped
/// to, so they fire only while this panel is on the focus path.
pub(super) const PANEL_KEY_CONTEXT: &str = "ResourcePanel";

const DOWN_KEY: &str = "down";
const UP_KEY: &str = "up";
pub(super) const ENTER_KEY: &str = "enter";
pub(super) const LEFT_KEY: &str = "left";
pub(super) const RIGHT_KEY: &str = "right";
pub(super) const SPACE_KEY: &str = "space";

/// [`PANEL_KEY_CONTEXT`] minus the filter box: Space types a space there, so
/// a key that is also text binds here instead. GPUI's `!` checks the whole
/// focus path, so this stops matching once the filter's `Input` is focused.
pub(super) const LIST_KEY_CONTEXT: &str = "ResourcePanel && !Input";

/// The cursor keys, bound directly in the panel's key context. Moving a selection
/// one row isn't something anyone picks from a palette, so these are the one kind
/// of key `keyboard-first.md` exempts from the command registry. Every real action -
/// open, collapse, expand, focus the filter - is a registered command instead (see
/// `actions::register_commands`), so it's in the palette too.
pub(super) fn panel_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new(DOWN_KEY, SelectNext, Some(PANEL_KEY_CONTEXT)),
        KeyBinding::new(UP_KEY, SelectPrevious, Some(PANEL_KEY_CONTEXT)),
    ]
}

/// Where the keyboard cursor sits: on a row the panel can open, or on a
/// Custom Resources subgroup header, which collapses and expands but opens
/// nothing - so it is not a [`NavTarget`].
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Cursor {
    Row(NavTarget),
    /// The subgroup header for this API group (`""` for core).
    Subgroup(String),
}

impl From<Navigable<'_>> for Cursor {
    fn from(item: Navigable<'_>) -> Self {
        match item {
            Navigable::Subgroup(group) => Cursor::Subgroup(group.to_string()),
            Navigable::Kind(kind) => Cursor::Row(NavTarget::Kind(kind.clone())),
        }
    }
}

/// Every stop currently visible, in the order it is drawn: fixed section
/// order, then each section's [`VisibleSection::navigable`] order. What
/// Up/Down step through - a collapsed or filtered-out section contributes
/// nothing, so its rows cannot be reached or opened until it is visible
/// again.
pub(super) fn visible_items(sections: &[VisibleSection]) -> Vec<Cursor> {
    sections
        .iter()
        .flat_map(VisibleSection::navigable)
        .map(Cursor::from)
        .collect()
}

/// The stop Down should select: the one after `current` in `visible`, the
/// first when nothing is selected (or the current selection is no longer
/// visible), or `current` unchanged when it is already the last - Down
/// never wraps.
pub(super) fn next(visible: &[Cursor], current: Option<&Cursor>) -> Option<Cursor> {
    if visible.is_empty() {
        return None;
    }
    let index = current.and_then(|current| visible.iter().position(|item| item == current));
    let next_index = match index {
        Some(index) => (index + 1).min(visible.len() - 1),
        None => 0,
    };
    Some(visible[next_index].clone())
}

/// The stop Up should select: the one before `current` in `visible`, the
/// first when nothing is selected (or it is no longer visible), or
/// `current` unchanged when it is already the first - Up never wraps.
pub(super) fn previous(visible: &[Cursor], current: Option<&Cursor>) -> Option<Cursor> {
    if visible.is_empty() {
        return None;
    }
    let index = current.and_then(|current| visible.iter().position(|item| item == current));
    let previous_index = match index {
        Some(index) => index.saturating_sub(1),
        None => 0,
    };
    Some(visible[previous_index].clone())
}

fn keystroke(literal: &str) -> Keystroke {
    Keystroke::parse(literal).expect("a literal keyboard hint parses")
}

/// The panel's shortcut hint row: every action section 1-4 add has a key, and
/// this is what makes that visible rather than merely true. Reads the live
/// keymap so a `keymap.toml` override is reflected here too, falling back to
/// the literal key `panel_bindings` registers when nothing is bound yet (the
/// first frame, before `cx.bind_keys` has run in a test).
pub(super) fn hint_row(window: &mut Window, cx: &App) -> impl IntoElement {
    let updown = div()
        .flex()
        .gap_0p5()
        .child(
            Kbd::binding_for_action(&SelectPrevious, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(keystroke(UP_KEY))),
        )
        .child(
            Kbd::binding_for_action(&SelectNext, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(keystroke(DOWN_KEY))),
        );
    let leftright = div()
        .flex()
        .gap_0p5()
        .child(
            Kbd::binding_for_action(&CollapseSection, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(keystroke(LEFT_KEY))),
        )
        .child(
            Kbd::binding_for_action(&ExpandSection, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(keystroke(RIGHT_KEY))),
        );
    let toggle = Kbd::binding_for_action(&ToggleSubgroup, Some(PANEL_KEY_CONTEXT), window)
        .unwrap_or_else(|| Kbd::new(keystroke(SPACE_KEY)));
    let enter = Kbd::binding_for_action(&OpenSelected, Some(PANEL_KEY_CONTEXT), window)
        .unwrap_or_else(|| Kbd::new(keystroke(ENTER_KEY)));
    let filter = Kbd::binding_for_action(
        &super::actions::FocusFilter,
        Some(PANEL_KEY_CONTEXT),
        window,
    )
    .unwrap_or_else(|| Kbd::new(keystroke("/")));

    div()
        .flex()
        .flex_wrap()
        .gap_3()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(hint(updown.into_any_element(), "Select"))
        .child(hint(enter.into_any_element(), "Open"))
        .child(hint(leftright.into_any_element(), "Collapse/expand"))
        .child(hint(toggle.into_any_element(), "Toggle group"))
        .child(hint(filter.into_any_element(), "Filter"))
}

fn hint(key: gpui_kit::AnyElement, label: &'static str) -> impl IntoElement {
    div().flex().items_center().gap_1().child(key).child(label)
}
