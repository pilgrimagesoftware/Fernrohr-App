//! `MainWindow`'s tab commands and `Cmd-W`: switching the focused tab group's
//! tab (`tab-keyboard-navigation`), and closing its displayed tab - asking
//! first when that would end a shell or discard an edit (Fernrohr#129) - or,
//! with no tab to close, the window (`per-tab-close-button`).
//!
//! Which group and tab are meant is [`crate::ui::panel::tabs`]'s call; this
//! module moves focus and dispatches, since the window owns the dock.

use super::*;
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;
use crate::ui::menu::CloseWindow;
use crate::ui::panel::tabs::{
    self, NextTab, PreviousTab, SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5,
    SelectTab6, SelectTab7, SelectTab8, SelectTab9, TabTarget,
};
use gpui_kit::component::dock::{ClosePanel, DockLayout, Panel};

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
    /// on screen - the window ([`Self::close_whole_window`]).
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
            let dock_area = dock_area.clone();
            let shown = tabs::focused_group(dock_area.read(cx), window, cx)
                .map(|group| group.panels[group.active_ix]);
            // A running shell or an unsaved edit asks first (Fernrohr#129).
            if let Some(id) = shown
                && super::arrange::ask_before_closing(&dock_area, id, window, cx)
            {
                return;
            }
            let area = dock_area.read(cx);
            if let Some(group) = tabs::focused_group(area, window, cx)
                && let Some(panel) = area.panel(group.panels[group.active_ix])
            {
                if tabs::is_only_panel(area, group.panels[group.active_ix]) {
                    // The tab group refuses to close the dock's last panel,
                    // so `ClosePanel` would do nothing; emptying the centre
                    // closes it, and the empty dock returns to the picker.
                    dock_area.update(cx, |area, cx| {
                        area.set_center(DockLayout::tabs(), window, cx)
                    });
                    return;
                }
                window.focus(&panel.focus_handle(cx), cx);
                window.dispatch_action(Box::new(ClosePanel), cx);
                return;
            }
        }
        self.close_whole_window(window, cx);
    }

    /// Closes the window - `Cmd-W` with no tab on screen - confirming first if
    /// that would drop a tunnel or discard an unsaved edit, irreversibly for an
    /// edit (Fernrohr#168). `close_window` removes the window without its
    /// should-close hook, so an edit is checked here too, not only by
    /// [`close_requested`]: with no panel on screen none should be open, but
    /// losing one silently is not a risk worth taking on that.
    pub(super) fn close_whole_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tunneled = self.contexts_losing_a_tunnel(cx);
        let unsaved = self.unsaved_edits(cx);
        if tunneled.is_empty() && unsaved.is_empty() {
            // Deferred: closing records the window's layout, which reads this
            // `MainWindow` - still mid-update here, so reading it now panics
            // (#135). Once this handler returns, the read is free.
            window.defer(cx, close_window);
        } else {
            open_close_window_dialog(&tunneled, unsaved, window, cx);
        }
    }

    /// What closing the window would discard for good: each panel's unsaved
    /// edit, as `Close Group` words it.
    fn unsaved_edits(&self, cx: &App) -> Vec<ConfirmText> {
        let WindowMode::Workspace { dock_area, .. } = &self.mode else {
            return Vec::new();
        };
        let area = dock_area.read(cx);
        tabs::all_panels(area)
            .into_iter()
            .filter_map(|panel| super::arrange::close_warning(area, panel, cx))
            .filter(|(_, severity)| *severity == Severity::Irreversible)
            .map(|(warning, _)| warning)
            .collect()
    }

    /// This window's contexts whose tunnel closing the window would tear
    /// down: bound to a live forward, and held by no other window.
    pub(super) fn contexts_losing_a_tunnel(&self, cx: &mut App) -> Vec<String> {
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

/// The window's close button: whether `window` may close now. With an unsaved
/// edit in any panel it may not - the irreversible "Close Window?" asks first
/// (Fernrohr#168), closing it only once confirmed. `Cmd-W` reaches the window
/// only with no panel on screen, so this is where an edit can be lost.
pub(super) fn close_requested(window: &mut Window, cx: &mut App) -> bool {
    let Some(Some(root)) = window.root::<Root>() else {
        return true;
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return true;
    };
    let unsaved = main_window.read(cx).unsaved_edits(cx);
    if unsaved.is_empty() {
        return true;
    }
    open_close_window_dialog(&[], unsaved, window, cx);
    false
}

/// "Close Window?" naming the contexts whose tunnel will disconnect - the
/// context bar's Disconnect dialog, with Close Window as the confirm - and each
/// unsaved edit it discards, which makes it irreversible.
fn open_close_window_dialog(
    tunneled: &[String],
    unsaved: Vec<ConfirmText>,
    window: &mut Window,
    cx: &mut App,
) {
    let severity = if unsaved.is_empty() {
        Severity::Recoverable
    } else {
        Severity::Irreversible
    };
    let lead = (!tunneled.is_empty()).then(|| close_window_confirmation_body(tunneled));
    let body = unsaved
        .into_iter()
        .fold(lead, |body, warning| match body {
            Some(body) => Some(body.text(" ").append(warning)),
            None => Some(warning),
        })
        .unwrap_or_default();
    let confirmation = Confirmation {
        title: "Close Window?".into(),
        body,
        confirm: "Close Window".into(),
        id_prefix: "close-window",
        severity,
    };
    confirm_dialog::open(confirmation, close_window, window, cx);
}

/// The confirmation's body: which contexts' tunnels will disconnect.
pub(super) fn close_window_confirmation_body(tunneled: &[String]) -> ConfirmText {
    let lead = match tunneled {
        [_] => "The tunnel for ",
        _ => "The tunnels for ",
    };
    ConfirmText::from(lead)
        .names(tunneled)
        .text(" will disconnect.")
}

/// Closes `panel` in the window `window` belongs to: the title bar close
/// control's route ([`crate::ui::panel_title::close_button`]).
///
/// By panel, not by the dock's `ClosePanel`, which follows focus and would
/// close the focused group's panel instead. `DockArea::remove_panel` also
/// removes the dock's last panel, which the tab group's own close refuses;
/// the empty dock then returns the window to the picker. Focus is handed on by
/// the window's `LayoutChanged` handling, as for `Cmd-W`.
pub(crate) fn close_panel<P: Panel>(panel: Entity<P>, window: &mut Window, cx: &mut App) {
    let Some(Some(root)) = window.root::<Root>() else {
        return;
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return;
    };
    let dock_area = match &main_window.read(cx).mode {
        WindowMode::Workspace { dock_area, .. } => dock_area.clone(),
        WindowMode::Picker(_) => return,
    };
    // A running shell or an unsaved edit asks first (Fernrohr#129).
    let id = gpui_kit::component::dock::PanelId::from(panel.entity_id());
    if super::arrange::ask_before_closing(&dock_area, id, window, cx) {
        return;
    }
    dock_area.update(cx, |area, cx| area.remove_panel(panel, window, cx));
}

#[cfg(test)]
mod tests;
