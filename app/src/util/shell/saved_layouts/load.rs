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

use super::*;

impl MainWindow {
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
    /// hold still restores today through the dock's own panel registry, the
    /// same as the automatic restore already does - `open_target_in`'s
    /// missing-context refusal (section 5's placeholder job) only applies to
    /// `Add` below, which is the one path that goes through it.
    pub(crate) fn load_replace(
        &mut self,
        layout: SavedLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            resource_panel,
            ..
        } = &self.mode
        else {
            return;
        };
        let dock_area = dock_area.clone();
        let resource_panel = resource_panel.clone();
        let SavedLayout {
            dock,
            resource_panel_width,
            window_width,
            window_height,
            ..
        } = layout;
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
        // `enter_workspace`'s own restored-branch pairing.
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
    /// ([`restored_panel_keys`], design.md D3) and opens each one through
    /// [`MainWindow::open_target_in`] - the one path every panel open already
    /// takes, so a panel matching one already open is focused instead of
    /// duplicated, and the window's Resource panel state and bounds are left
    /// untouched. A no-op for a window that isn't in `Workspace` mode.
    pub(crate) fn load_add(
        &mut self,
        layout: SavedLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.mode, WindowMode::Workspace { .. }) {
            return;
        }
        for key in restored_panel_keys(&layout.dock.center) {
            // Section 5: a panel whose context this window doesn't hold is
            // refused here exactly as `open_target_in` already refuses any
            // other request for a context the window doesn't hold - a later
            // change restores it as a placeholder instead of silently
            // dropping it (design.md D5). `None` (an unrecognised panel kind,
            // or one `panel_key` couldn't read a required field from) is
            // skipped the same way an unrestored panel already is on the
            // automatic restore - there is no key here to open anything with.
            let Some(key) = key else {
                continue;
            };
            self.open_target_in(
                key.target,
                None,
                Some(key.context_name),
                key.namespaces,
                OpenMode::Foreground,
                window,
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests;
