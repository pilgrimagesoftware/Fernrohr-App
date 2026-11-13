//! Disconnect All and Disconnect Others (#359): one Recoverable confirmation
//! naming every context it disconnects, from a capsule's menu or the Context
//! menu and palette. Confirming disconnects each through
//! `MainWindow::disconnect_context`, the single-context Disconnect's path, so
//! panels close and holds release exactly as they do one at a time.

use super::StatusBarView;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::util::context_lifecycle;
use gpui_kit::*;

/// Which of the window's contexts a multi-disconnect takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Disconnecting {
    All,
    /// Every context but this one.
    OthersThan(String),
}

impl Disconnecting {
    /// Of the window's `contexts`, the ones this disconnects, in the window's
    /// order.
    pub(crate) fn of(&self, contexts: &[String]) -> Vec<String> {
        contexts
            .iter()
            .filter(|context| match self {
                Disconnecting::All => true,
                Disconnecting::OthersThan(kept) => *context != kept,
            })
            .cloned()
            .collect()
    }

    fn title(&self) -> &'static str {
        match self {
            Disconnecting::All => "Disconnect All Contexts?",
            Disconnecting::OthersThan(_) => "Disconnect Other Contexts?",
        }
    }

    fn confirm(&self) -> &'static str {
        match self {
            Disconnecting::All => "Disconnect All",
            Disconnecting::OthersThan(_) => "Disconnect Others",
        }
    }
}

impl StatusBarView {
    /// Asks before disconnecting `which` of the window's contexts. One context
    /// asks the single Disconnect's question, naming its panels; none asks
    /// nothing.
    pub(crate) fn open_disconnect_many_dialog(
        &mut self,
        which: Disconnecting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let names = which.of(&self.context_names);
        match names.as_slice() {
            [] => return,
            [only] => return self.open_disconnect_dialog(only.clone(), window, cx),
            _ => {}
        }
        let Some(main_window) = self.main_window.upgrade() else {
            return;
        };
        let panel_count = names
            .iter()
            .map(|name| main_window.read(cx).context_panel_count(name))
            .sum();
        let staying: Vec<String> = names
            .iter()
            .filter(|name| ClusterRegistry::holder_count(cx, name) > 1)
            .cloned()
            .collect();
        let confirmation = Confirmation {
            title: which.title().into(),
            body: context_lifecycle::disconnect_many_confirmation_body(
                &names,
                panel_count,
                &staying,
            ),
            confirm: which.confirm().into(),
            id_prefix: "context-disconnect-many",
            severity: Severity::Recoverable,
        };
        let target = self.main_window.clone();
        confirm_dialog::open(
            confirmation,
            move |window, cx| {
                let _ = target.update(cx, |main_window, cx| {
                    for name in &names {
                        main_window.disconnect_context(name.clone(), window, cx);
                    }
                });
            },
            window,
            cx,
        );
    }

    /// How many contexts the window holds.
    pub(super) fn context_count(&self) -> usize {
        self.context_names.len()
    }
}

#[cfg(test)]
mod tests {
    use super::Disconnecting;

    #[test]
    fn all_takes_every_context_and_others_keeps_one() {
        let contexts = ["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(Disconnecting::All.of(&contexts), contexts);
        assert_eq!(
            Disconnecting::OthersThan("b".into()).of(&contexts),
            ["a".to_string(), "c".to_string()]
        );
    }
}
