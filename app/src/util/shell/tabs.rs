//! `MainWindow`'s tab commands and `Cmd-W`: switching the focused tab group's
//! tab (`tab-keyboard-navigation`), and closing its displayed tab - or, with
//! no tab to close, the window (`per-tab-close-button`).
//!
//! Which group and tab are meant is [`crate::ui::panel::tabs`]'s call; this
//! module moves focus and dispatches, since the window owns the dock.

use super::*;
use crate::ui::menu::CloseWindow;
use crate::ui::panel::tabs::{
    self, NextTab, PreviousTab, SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5,
    SelectTab6, SelectTab7, SelectTab8, SelectTab9, TabTarget,
};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::dock::ClosePanel;

impl MainWindow {
    /// The tab commands' listeners, and `Cmd-W`'s, for the window's root
    /// element. Listening here rather than in the menu's app-wide handler is
    /// what lets `Cmd-W` close a tab in a main window while it still closes
    /// Tunnels or About, which have no tabs.
    pub(super) fn with_tab_actions(element: Div, cx: &mut Context<Self>) -> Div {
        let show = |target| {
            move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                this.show_tab(target, window, cx)
            }
        };
        let next = show(TabTarget::Next);
        let previous = show(TabTarget::Previous);
        let positions: [_; 9] = std::array::from_fn(|ix| show(TabTarget::Position(ix + 1)));
        let [p1, p2, p3, p4, p5, p6, p7, p8, p9] = positions;
        element
            .on_action(cx.listener(move |this, _: &NextTab, w, cx| next(this, w, cx)))
            .on_action(cx.listener(move |this, _: &PreviousTab, w, cx| previous(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab1, w, cx| p1(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab2, w, cx| p2(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab3, w, cx| p3(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab4, w, cx| p4(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab5, w, cx| p5(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab6, w, cx| p6(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab7, w, cx| p7(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab8, w, cx| p8(this, w, cx)))
            .on_action(cx.listener(move |this, _: &SelectTab9, w, cx| p9(this, w, cx)))
            .on_action(cx.listener(Self::on_action_close_window))
    }

    /// Shows `target` in the focused tab group and focuses it - the same
    /// select-then-focus pair `open_target_with_view` uses to re-show a panel.
    fn show_tab(&mut self, target: TabTarget, window: &mut Window, cx: &mut Context<Self>) {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return;
        };
        let dock_area = dock_area.clone();
        let Some(group) = tabs::focused_group(dock_area.read(cx), window, cx) else {
            return;
        };
        let Some(ix) = tabs::target_index(group.panels.len(), group.active_ix, target) else {
            return;
        };
        let id = group.panels[ix];
        dock_area.update(cx, |area, cx| area.select_panel(id, window, cx));
        if let Some(panel) = dock_area.read(cx).panel(id) {
            window.focus(&panel.focus_handle(cx), cx);
        }
    }

    /// `Cmd-W`: closes the focused tab group's displayed tab, or - with no tab
    /// on screen - the window, confirming first if that would drop a tunnel.
    ///
    /// The tab group handles `ClosePanel`, so it reaches a group only along the
    /// focus path. Focusing the displayed tab first is what makes `Cmd-W` close
    /// something with focus in the Resource panel; dispatching reads the focus
    /// as of this call and resolves it against the frame already drawn, where
    /// that tab is on screen.
    fn on_action_close_window(
        &mut self,
        _: &CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WindowMode::Workspace { dock_area, .. } = &self.mode {
            let area = dock_area.read(cx);
            if let Some(group) = tabs::focused_group(area, window, cx)
                && let Some(panel) = area.panel(group.panels[group.active_ix])
            {
                window.focus(&panel.focus_handle(cx), cx);
                window.dispatch_action(Box::new(ClosePanel), cx);
                return;
            }
        }
        let tunneled = self.contexts_losing_a_tunnel(cx);
        if tunneled.is_empty() {
            close_window(window, cx);
        } else {
            open_close_window_dialog(tunneled, window, cx);
        }
    }

    /// This window's contexts whose tunnel closing the window would tear
    /// down: bound to a live forward, and held by no other window.
    fn contexts_losing_a_tunnel(&self, cx: &mut App) -> Vec<String> {
        let WindowMode::Workspace { contexts, .. } = &self.mode else {
            return Vec::new();
        };
        let holders: Vec<usize> = contexts
            .iter()
            .map(|context_name| ClusterRegistry::holder_count(cx, context_name))
            .collect();
        losing_a_tunnel(contexts, &holders, |context_name| {
            // The window holds `context_name`, so its session exists;
            // `connection` finds it rather than making one.
            ClusterRegistry::connection(cx, context_name)
                .read(cx)
                .forward_state()
                .is_some_and(|state| {
                    *state.borrow() != crate::forward::managed::ForwardState::Disconnected
                })
        })
    }
}

/// Of `contexts` (with `holders[i]` windows holding `contexts[i]`), the ones
/// this window alone holds whose forward `is_live` - closing the window
/// releases the last hold, and with it the tunnel. A context another window
/// still holds keeps its tunnel, so it needs no warning.
pub(super) fn losing_a_tunnel(
    contexts: &[String],
    holders: &[usize],
    mut is_live: impl FnMut(&str) -> bool,
) -> Vec<String> {
    contexts
        .iter()
        .zip(holders)
        .filter(|(_, holders)| **holders == 1)
        .filter(|(context_name, _)| is_live(context_name))
        .map(|(context_name, _)| context_name.clone())
        .collect()
}

/// "Close Window?" naming the contexts whose tunnel will disconnect - the
/// context bar's Disconnect dialog, with Close Window as the confirm.
fn open_close_window_dialog(tunneled: Vec<String>, window: &mut Window, cx: &mut App) {
    let body = close_window_confirmation_body(&tunneled);
    Root::update(window, cx, |root, window, cx| {
        root.open_dialog(
            move |dialog, _window, _cx| {
                dialog.title("Close Window?").child(body.clone()).footer(
                    DialogFooter::new()
                        .child(Button::new("close-window-cancel").label("Cancel").on_click(
                            |_event, window, cx| {
                                Root::update(window, cx, |root, window, cx| {
                                    root.close_dialog(window, cx);
                                });
                            },
                        ))
                        .child(
                            Button::new("close-window-confirm")
                                .label("Close Window")
                                .with_variant(gpui_kit::component::button::ButtonVariant::Danger)
                                .on_click(|_event, window, cx| {
                                    Root::update(window, cx, |root, window, cx| {
                                        root.close_dialog(window, cx);
                                    });
                                    close_window(window, cx);
                                }),
                        ),
                )
            },
            window,
            cx,
        );
    });
}

/// The confirmation's body: which contexts' tunnels will disconnect.
pub(super) fn close_window_confirmation_body(tunneled: &[String]) -> String {
    match tunneled {
        [context_name] => format!("The tunnel for {context_name} will disconnect."),
        names => format!("The tunnels for {} will disconnect.", names.join(", ")),
    }
}

#[cfg(test)]
mod tests;
