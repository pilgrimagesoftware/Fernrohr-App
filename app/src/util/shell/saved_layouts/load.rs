//! Applying a [`SavedLayout`] to the current window: `saved_layouts.
//! load_replace` and `saved_layouts.load_add` (`saved-panel-layouts` tasks
//! section 4, design.md D4). Both are reached through `ui::picker::
//! saved_layouts::SavedLayoutsPicker`'s `selected()`/`main_window()` seam -
//! the picker closes its own dialog and calls [`MainWindow::load_replace`]/
//! [`MainWindow::load_add`] directly, so this module never touches the
//! picker's own state, only `MainWindow`'s.
//!
//! **Replace** rebuilds the window's dock wholesale via [`DockArea::load`] -
//! the same rebuild path [`super::main_window::MainWindow::enter_workspace`]'s
//! own restored branch already uses when a window opens on a saved
//! `dock-layouts.json` arrangement - then applies the saved Resource panel
//! width/visibility and window size. **Add** decodes the saved layout's
//! panels into [`PanelKey`]s the same way (`panels.rs`'s
//! [`restored_panel_keys`], design.md D3) and opens each one through
//! [`MainWindow::open_target_in`], which already dedups by content key and
//! leaves every other window state untouched.
//!
//! **Section 5 (missing context handling, design.md D5):** both modes refuse
//! to build a saved panel scoped to a context this window doesn't hold -
//! building that panel's own kind (`PodsPanel::new` and every other kind
//! alike) reads its connection through `ClusterRegistry::connection`, which
//! connects a context lazily on first use (`ensure_init`) rather than
//! refusing one it doesn't know. Left alone, `DockArea::load` would do that
//! silently for Replace, and would do it for Add too once a key decodes.
//! Both instead restore that one panel as `ui::unrestored`'s placeholder, in
//! the same slot, via [`placeholder_missing_contexts`] (Replace, which
//! transforms the whole saved tree before `DockArea::load` ever sees it) and
//! the `load_add` loop below (which checks each decoded key's context before
//! opening it). A panel `restored_panel_keys` itself can't key - an
//! unrecognised kind, or state it can't read - is untouched by either: that
//! is `DockArea::load`'s own registry fallback's job already (section 5.3),
//! and `load_add` has never opened one of those (there is no key to open).

use super::*;
use crate::ui::unrestored::{self, UnrestoredPanel};
use gpui_kit::component::dock::panel_handle;

/// Which of the two ways to load a saved layout: the picker's Enter and
/// Secondary-Enter, and `agent-mcp`'s `load_layout` tool's `mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadMode {
    /// [`MainWindow::load_replace`].
    Replace,
    /// [`MainWindow::load_add`].
    Add,
}

/// What a [`MainWindow::load_layout`] restored as placeholders rather than
/// panels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LoadedLayout {
    /// The context of each saved panel restored as a placeholder because this
    /// window doesn't hold it, in the layout's own panel order (one entry per
    /// panel, so a context can repeat).
    pub(crate) placeholder_contexts: Vec<String>,
}

impl MainWindow {
    /// Loads `layout` in `mode`, and reports which of its panels became
    /// missing-context placeholders. Both modes decide that by the same rule:
    /// a panel whose key decodes, scoped to a context outside this window's
    /// `contexts` - so the report is read off the layout before loading it,
    /// rather than off the dock after. Nothing connects: the window's
    /// `contexts` are what both modes keep.
    pub(crate) fn load_layout(
        &mut self,
        layout: SavedLayout,
        mode: LoadMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> LoadedLayout {
        let WindowMode::Workspace { contexts, .. } = &self.mode else {
            return LoadedLayout::default();
        };
        let placeholder_contexts = restored_panel_leaves(&layout.dock.center)
            .into_iter()
            .filter_map(|(_, key)| key)
            .map(|key| key.context_name)
            .filter(|context_name| !contexts.contains(context_name))
            .collect();
        match mode {
            LoadMode::Replace => self.load_replace(layout, window, cx),
            LoadMode::Add => self.load_add(layout, window, cx),
        }
        LoadedLayout {
            placeholder_contexts,
        }
    }

    /// `saved_layouts.load_replace` (design.md D4): "I asked for *that*
    /// layout" - a wholesale swap, not a content-only merge. Rebuilds this
    /// window's dock from `layout.dock` via [`DockArea::load`], refreshes
    /// `open_panels`/`last_focused_panel` to match the rebuilt tree the same
    /// way a restored window's `enter_workspace` does, applies the saved
    /// Resource panel width/visibility and window size, and focuses the
    /// rebuilt dock's first panel (or the Resource panel with none open) -
    /// the same baseline a window lands on when it first opens. A no-op for
    /// a window that left `Workspace` mode between the command firing and
    /// this running (closed or disconnected while the picker was open).
    ///
    /// `DockArea::load` itself emits `DockEvent::LayoutChanged`, which the
    /// `watch_workspace` subscription already wired at `enter_workspace` time
    /// turns into exactly the bookkeeping a dock change always gets -
    /// `save_dock_layout`, `keep_focus_on_a_panel`, `forget_closed_panels` -
    /// so this method does not call any of those itself; it only has to make
    /// `open_panels` correct before that subscription's effects flush, which
    /// assigning it below (rather than inside a callback) already guarantees.
    ///
    /// Keeps the window's existing `contexts`: Replace swaps the
    /// *arrangement*, not which clusters the window is connected to
    /// (design.md Non-Goals - connecting a context on demand isn't this
    /// change's job). A saved panel scoped to a context this window doesn't
    /// hold is swapped for `ui::unrestored`'s placeholder before the dock
    /// ever sees it ([`placeholder_missing_contexts`], section 5.1) - every
    /// other panel restores through the dock's own panel registry exactly as
    /// the automatic restore already does.
    pub(crate) fn load_replace(
        &mut self,
        layout: SavedLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            resource_panel,
            contexts,
            ..
        } = &self.mode
        else {
            return;
        };
        let dock_area = dock_area.clone();
        let resource_panel = resource_panel.clone();
        let contexts = contexts.clone();
        let SavedLayout {
            mut dock,
            resource_panel_width,
            window_width,
            window_height,
            ..
        } = layout;
        // Section 5.1: swapped in before `restored_panel_keys` reads the tree
        // and before `area.load` builds it, so both the key pairing below and
        // the dock itself agree on which slots are placeholders.
        dock.center = placeholder_missing_contexts(dock.center, &contexts);
        // Computed before `dock` moves into the closure below - the same
        // `restored_panel_keys` decoder `enter_workspace`'s own restored
        // branch reads off a saved dock's `center` (design.md D3).
        let restored_keys = restored_panel_keys(&dock.center);
        dock_area.update(cx, |area, cx| {
            area.load(dock, window, cx)
                .expect("a saved layout's own dock dump must load");
        });
        let ids = dock_area
            .read(cx)
            .layout(DockPlacement::Center)
            .map(|tree| tree.panels().collect::<Vec<_>>())
            .unwrap_or_default();
        // Paired by position, one key slot per rebuilt panel - exactly
        // `enter_workspace`'s own restored-branch pairing. A placeholder's
        // slot has no key (its panel name doesn't match anything
        // `panel_key` recognises), so it is built in the dock but left out
        // of `open_panels` - the same treatment any other unkeyable restored
        // panel already gets.
        let open_panels: Vec<OpenPanel> = ids
            .into_iter()
            .zip(restored_keys)
            .filter_map(|(id, key)| Some((id, key?)))
            .map(|(id, key)| OpenPanel {
                key,
                id,
                // Restored, not built here - no entity this window held.
                panel: None,
                group: crate::ui::panel::tabs::group_of(dock_area.read(cx), id),
                _focus_watch: Self::watch_panel_focus(&dock_area, id, window, cx),
            })
            .collect();
        let new_resource_width = resource_panel_width
            .map(px)
            .map(|width| width.clamp(RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_MAX_WIDTH));
        if let WindowMode::Workspace {
            open_panels: current_panels,
            resource_width: current_width,
            resource_collapsed,
            last_focused_panel,
            nav,
            ..
        } = &mut self.mode
        {
            *current_panels = open_panels;
            *last_focused_panel = None;
            // No "current nav" survives a wholesale swap - the same baseline
            // a window starts on when it opens.
            **nav = NavTarget::pods();
            match new_resource_width {
                Some(width) => {
                    *current_width = width;
                    *resource_collapsed = false;
                }
                // `None` is hidden (design.md D2's same meaning as
                // `WindowLayout.resource_panel_width`); the stored pixel
                // width is irrelevant while collapsed, so it's left as-is
                // rather than reset to a default nothing will read.
                None => *resource_collapsed = true,
            }
        }
        resource_panel.update(cx, |panel, cx| {
            panel.set_selected(Some(NavTarget::pods()), cx)
        });
        // `restorable_bounds`'s own frame/content split (`layout.rs`) only
        // matters when *saving* a position back out of a live window; a
        // `SavedLayout` carries no `x`/`y` to restore (design.md D2's
        // `SavedLayout` has no position fields, only `window_width`/
        // `window_height`) - GPUI's cross-platform window API has no "move
        // to this exact point" call a saved layout could feed back into
        // regardless, so Replace resizes the window in place rather than
        // repositioning it; see this change's final report for what's
        // possible per backend.
        window.resize(size(px(window_width), px(window_height)));
        self.focus_displayed_panel(window, cx);
        cx.notify();
    }

    /// `saved_layouts.load_add` (design.md D4): additive only, never a
    /// wholesale swap. Decodes `layout`'s panels into [`PanelKey`]s
    /// ([`restored_panel_leaves`], design.md D3) and opens each one whose
    /// context this window holds through [`MainWindow::open_target_in`] - the
    /// one path every panel open already takes, so a panel matching one
    /// already open is focused instead of duplicated, and the window's
    /// Resource panel state and bounds are left untouched. A saved panel
    /// scoped to a context this window doesn't hold is restored as
    /// `ui::unrestored`'s placeholder instead (section 5.1), added straight
    /// to the dock rather than through `open_target_in` - which would only
    /// refuse it the same way and build nothing. A no-op for a window that
    /// isn't in `Workspace` mode.
    pub(crate) fn load_add(
        &mut self,
        layout: SavedLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            contexts,
            ..
        } = &self.mode
        else {
            return;
        };
        let dock_area = dock_area.clone();
        let contexts = contexts.clone();
        for (state, key) in restored_panel_leaves(&layout.dock.center) {
            // `None` (an unrecognised panel kind, or one `panel_key`
            // couldn't read a required field from) is skipped the same way
            // an unrestored panel already is on the automatic restore -
            // there is no key here to open anything with, or to name a
            // missing context by.
            let Some(key) = key else {
                continue;
            };
            if contexts.contains(&key.context_name) {
                self.open_target_in(
                    key.target,
                    None,
                    Some(key.context_name),
                    key.namespaces,
                    OpenMode::Foreground,
                    window,
                    cx,
                );
                continue;
            }
            // Section 5.1: refused here exactly as `open_target_in` already
            // refuses any other request for a context the window doesn't
            // hold (design.md D5) - but restored as a placeholder in the
            // dock rather than silently dropped. Not given an `open_panels`
            // entry: there is no real panel behind it to dedupe a later
            // request against or to select, the same treatment a slot
            // `restored_panel_keys` can't key already gets in Replace above.
            let reason = format!("this window isn't connected to {}", key.context_name);
            let placeholder = cx.new(|cx| UnrestoredPanel::new(state, reason, cx));
            dock_area.update(cx, |area, cx| {
                area.add_panel_view(
                    panel_handle(placeholder),
                    DockPlacement::Center,
                    None,
                    window,
                    cx,
                );
            });
        }
    }
}

/// Swaps every panel in `state`'s tree whose saved `context_name` isn't in
/// `contexts` for `ui::unrestored`'s placeholder, in the same tree position
/// (section 5.1, design.md D5). Building that panel's own kind here would
/// silently connect a context the user never asked this window to hold -
/// `ClusterRegistry::connection`'s `ensure_init` does exactly that on first
/// use, which is the same thing `open_target_in`'s own refusal already
/// exists to prevent for every other open. A leaf `restored_panel_keys`
/// itself can't key - an unrecognised kind, or state it can't read - is left
/// untouched: `DockArea::load`'s own registry fallback already turns that
/// into a placeholder (section 5.3), so this only has to handle the one case
/// that fallback cannot reach - a kind this build recognises, scoped to a
/// context this window simply isn't connected to right now.
fn placeholder_missing_contexts(mut state: PanelState, contexts: &[String]) -> PanelState {
    if matches!(state.info, PanelInfo::Panel(_)) {
        let key = restored_panel_keys(&state).into_iter().next().flatten();
        return match key {
            Some(key) if !contexts.contains(&key.context_name) => {
                missing_context_placeholder(state, &key.context_name)
            }
            _ => state,
        };
    }
    state.children = state
        .children
        .into_iter()
        .map(|child| placeholder_missing_contexts(child, contexts))
        .collect();
    state
}

/// Wraps `state` (a panel kind this build recognises, scoped to
/// `context_name`) as a saved [`unrestored::PANEL_NAME`] panel:
/// [`unrestored::register_restore`] unwraps it straight back to an
/// [`UnrestoredPanel`] naming why, with `state` kept verbatim as its own
/// `dump` - so a later load in a window that holds `context_name` restores
/// the real panel, not this placeholder.
fn missing_context_placeholder(state: PanelState, context_name: &str) -> PanelState {
    PanelState {
        panel_name: unrestored::PANEL_NAME.to_string(),
        children: Vec::new(),
        info: PanelInfo::Panel(serde_json::json!({
            "reason": format!("this window isn't connected to {context_name}"),
            "original": state,
        })),
    }
}

#[cfg(test)]
mod tests;
