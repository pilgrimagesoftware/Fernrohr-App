//! Opening a shell panel (`k9s-remaining-keybindings` 3): the Pods panel picks
//! the pod and container and asks for it through [`OpenExecSession`]; the window
//! opens it like any other panel, so it docks, titles and closes the same way.

use super::MainWindow;
use crate::k8s::resource::exec::ExecTarget;
use crate::ui::nav::{NavTarget, OpenMode};
use gpui_kit::*;

/// Asks the window to open a shell in `target`, on `context_name`'s cluster.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = shell, no_json)]
pub struct OpenExecSession {
    pub context_name: String,
    pub target: ExecTarget,
}

impl MainWindow {
    pub(super) fn on_action_open_exec(
        &mut self,
        action: &OpenExecSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target_in(
            NavTarget::Exec(action.target.clone()),
            None,
            Some(action.context_name.clone()),
            Vec::new(),
            OpenMode::Foreground,
            window,
            cx,
        );
    }
}

/// Opens a session in `target` on `context_name`'s cluster in the main window
/// `window` holds - from wherever the request comes, a dialog or another
/// window's flow included, so it needs no focus path to the window.
pub(crate) fn open_exec(
    context_name: String,
    target: ExecTarget,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(Some(root)) = window.root::<gpui_kit::component::Root>() else {
        return;
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return;
    };
    let action = OpenExecSession {
        context_name,
        target,
    };
    // Deferred: the caller is mid-update in a panel's action, or a dialog's.
    window.defer(cx, move |window, cx| {
        main_window.update(cx, |main_window, cx| {
            main_window.on_action_open_exec(&action, window, cx)
        });
    });
}

#[cfg(test)]
mod tests;
