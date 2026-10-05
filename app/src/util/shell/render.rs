//! Drawing the main window, and opening the command palette over it.

use super::*;

/// Opens the command palette in a dialog on `window`'s `Root`. A fresh
/// `CommandState` is created per open (not reused across opens) since the
/// palette's own query/selection state should reset each time it's summoned.
pub(super) fn open_command_palette(window: &mut Window, cx: &mut App) {
    crate::util::palette::open(window, cx);
}

impl MainWindow {
    /// The Resource panel beside the dock, on `side`. Expanded, it's a resizable
    /// split - drag the divider to trade list width for dock space - with the panel
    /// first on the left or last on the right; collapsed, a strip with the button
    /// that brings it back, on the same edge.
    fn workspace_row(
        dock_area: &Entity<DockArea>,
        resource_panel: &Entity<crate::ui::resource_panel::ResourcePanel>,
        width: Pixels,
        side: crate::ui::resource_panel::ResourceSide,
        collapsed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui::resource_panel::{ResourceSide, collapsed_strip};
        // An empty centre - the last panel closed, the window still connected -
        // shows the key that opens the next kind instead of a blank area.
        let dock = if dock_area.read(cx).is_empty(DockPlacement::Center, cx) {
            empty_dock_hint(window, cx)
        } else {
            // The arrange commands' context: on the focus path while a panel in
            // the dock has focus, not while a dialog or the Resource panel does.
            div()
                .size_full()
                .key_context(crate::ui::panel::arrange::DOCK_KEY_CONTEXT)
                .child(dock_area.clone())
                .into_any_element()
        };
        if collapsed {
            let strip = collapsed_strip(side, cx).into_any_element();
            let dock = div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(dock)
                .into_any_element();
            let (first, second) = match side {
                ResourceSide::Left => (strip, dock),
                ResourceSide::Right => (dock, strip),
            };
            return div()
                .size_full()
                .flex()
                .child(first)
                .child(second)
                .into_any_element();
        }
        let panel = resizable_panel()
            .size(width)
            .size_range(RESOURCE_PANEL_MIN_WIDTH..RESOURCE_PANEL_MAX_WIDTH)
            .child(resource_panel.clone());
        let this = cx.weak_entity();
        let split = h_resizable("workspace-split").on_resize(move |state, _window, cx| {
            // The panel's size is the split's first on the left, its last on the right.
            let sizes = state.read(cx).sizes();
            let width = match side {
                ResourceSide::Left => sizes.first(),
                ResourceSide::Right => sizes.last(),
            };
            let Some(width) = width.copied() else {
                return;
            };
            let _ = this.update(cx, |this, _cx| this.set_resource_width(width));
        });
        match side {
            ResourceSide::Left => split.child(panel).child(resizable_panel().child(dock)),
            ResourceSide::Right => split.child(resizable_panel().child(dock)).child(panel),
        }
        .into_any_element()
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = match &self.mode {
            WindowMode::Picker(picker) => picker.clone().into_any_element(),
            // The Resource panel is the window's left edge. It used to be a
            // fixed Pods/Logs list built here; the kinds now come from the
            // cluster's own discovery (spec 8.1), so it owns its own chrome.
            // `connection-status-bar`: the workspace is a column - the panel row
            // at `flex_1`, then the status bar fixed to its own height below it.
            // The picker has no status bar: it shows its own connect progress.
            WindowMode::Workspace {
                dock_area,
                resource_panel,
                status_bar,
                resource_width,
                resource_side,
                resource_collapsed,
                ..
            } => div()
                .size_full()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h_0().child(Self::workspace_row(
                    dock_area,
                    resource_panel,
                    *resource_width,
                    *resource_side,
                    *resource_collapsed,
                    window,
                    cx,
                )))
                .child(status_bar.clone())
                .into_any_element(),
        };
        // The window's own top bar, then the mode's body below it.
        let toolbar = crate::ui::toolbar::window_toolbar(window, cx).into_any_element();
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle);
        Self::with_arrange_actions(Self::with_tab_actions(root, cx), cx)
            .on_action(|_: &ToggleCommandPalette, window, cx| {
                open_command_palette(window, cx);
            })
            .on_action(|_: &crate::util::key_hints::ShowKeyHints, window, cx| {
                crate::util::key_hints::open(window, cx);
            })
            .on_action(cx.listener(Self::on_action_show_pods))
            .on_action(cx.listener(Self::on_action_show_events))
            .on_action(cx.listener(Self::on_action_focus_resources))
            .on_action(cx.listener(Self::on_action_warp_context))
            .on_action(cx.listener(Self::on_action_open_exec))
            .on_action(cx.listener(Self::on_action_switch_namespace_set))
            .on_action(cx.listener(Self::on_action_switch_context_namespace_set))
            .on_action(cx.listener(Self::on_action_create_namespace_set))
            .on_action(cx.listener(Self::on_action_edit_namespace_set))
            .on_action(cx.listener(Self::on_action_remove_namespace_set))
            .on_action(cx.listener(Self::on_action_apply_namespace_set))
            .on_action(cx.listener(Self::on_action_refresh_resources))
            .on_action(cx.listener(Self::on_action_focus_next_panel))
            .on_action(cx.listener(Self::on_action_focus_previous_panel))
            .on_action(cx.listener(Self::on_action_show_logs))
            .on_action(cx.listener(Self::on_action_show_pod_detail))
            .on_action(cx.listener(Self::on_action_show_pod_detail_yaml))
            .on_action(cx.listener(Self::on_action_set_tunnel))
            .on_action(cx.listener(Self::on_action_follow_reference))
            .on_action(cx.listener(Self::on_action_open_listed_object))
            .on_action(cx.listener(Self::on_action_toggle_resource_panel))
            .on_action(cx.listener(Self::on_action_move_resource_panel))
            .on_action(cx.listener(Self::on_action_add_context))
            .on_action(cx.listener(Self::on_action_disconnect_active_context))
            .on_action(cx.listener(Self::on_action_save_resource_side))
            .child(toolbar)
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod toolbar_tests;
