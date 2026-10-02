//! Drawing the main window, and opening the command palette over it.

use super::*;

/// Opens the command palette in a dialog on `window`'s `Root`. A fresh
/// `CommandState` is created per open (not reused across opens) since the
/// palette's own query/selection state should reset each time it's summoned.
pub(super) fn open_command_palette(window: &mut Window, cx: &mut App) {
    crate::util::palette::open(window, cx);
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = match &self.mode {
            WindowMode::Picker(picker) => picker.clone().into_any_element(),
            // The Resource panel is the window's left edge. It used to be a
            // fixed Pods/Logs list built here; the kinds now come from the
            // cluster's own discovery (spec 8.1), so it owns its own chrome.
            // `connection-status-bar`/`window-context-bar`: the workspace is a
            // column - the context bar fixed to its own height at the top, the
            // existing panel row at `flex_1`, then the status bar fixed to its
            // own height below it. The picker has neither bar (it shows its own
            // connect progress instead, and has no context list to chip yet).
            WindowMode::Workspace {
                dock_area,
                resource_panel,
                status_bar,
                context_bar,
                resource_width,
                ..
            } => div()
                .size_full()
                .flex()
                .flex_col()
                .child(context_bar.clone())
                .child(
                    // The Resource panel is a resizable split, not a fixed-width
                    // column: drag the divider to trade list width for dock space.
                    div().flex_1().min_h_0().child(
                        h_resizable("workspace-split")
                            .on_resize({
                                let this = cx.weak_entity();
                                move |state, _window, cx| {
                                    let Some(width) = state.read(cx).sizes().first().copied()
                                    else {
                                        return;
                                    };
                                    let _ = this.update(cx, |this, _cx| {
                                        this.set_resource_width(width);
                                    });
                                }
                            })
                            .child(
                                resizable_panel()
                                    .size(*resource_width)
                                    .size_range(RESOURCE_PANEL_MIN_WIDTH..RESOURCE_PANEL_MAX_WIDTH)
                                    .child(resource_panel.clone()),
                            )
                            .child(resizable_panel().child(dock_area.clone().into_any_element())),
                    ),
                )
                .child(status_bar.clone())
                .into_any_element(),
        };
        let root = div().size_full().track_focus(&self.focus_handle);
        Self::with_tab_actions(root, cx)
            .on_action(|_: &ToggleCommandPalette, window, cx| {
                open_command_palette(window, cx);
            })
            .on_action(cx.listener(Self::on_action_show_pods))
            .on_action(cx.listener(Self::on_action_focus_resources))
            .on_action(cx.listener(Self::on_action_focus_next_panel))
            .on_action(cx.listener(Self::on_action_focus_previous_panel))
            .on_action(cx.listener(Self::on_action_show_logs))
            .on_action(cx.listener(Self::on_action_show_pod_detail))
            .on_action(cx.listener(Self::on_action_show_pod_detail_yaml))
            .on_action(cx.listener(Self::on_action_set_tunnel))
            .on_action(cx.listener(Self::on_action_follow_reference))
            .child(body)
    }
}

#[cfg(test)]
mod tests;
