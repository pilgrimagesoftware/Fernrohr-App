//! Shell into the selected pod (`k9s-remaining-keybindings` 3.2, 3.3): `s` opens
//! a shell in its running container - asking which, when it runs more than one.
//!
//! The command is only offered for a pod with a running container: while one is
//! selected, the table sits inside a [`SHELL_KEY_CONTEXT`] element, which is the
//! context the command is registered in, so the palette and the key leave it
//! out otherwise.

use super::*;
use crate::k8s::resource::exec::ExecTarget;
use crate::util::shell::OpenExecSession;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;

/// The context the shell command lives in, present only while the selected
/// pod has a running container.
pub const SHELL_KEY_CONTEXT: &str = "PodShellable";

/// The container picker's buttons: one per running container, and Cancel.
pub(super) fn container_button_id(container: &str) -> SharedString {
    format!("pods-shell-container-{container}").into()
}
pub(super) const CANCEL_SHELL_ID: &str = "pods-shell-cancel";

/// `pod`'s running containers, in the order its spec lists them.
pub(crate) fn running_containers(pod: &Pod) -> Vec<String> {
    let statuses = pod
        .status
        .as_ref()
        .and_then(|status| status.container_statuses.as_deref())
        .unwrap_or_default();
    pod.spec
        .iter()
        .flat_map(|spec| &spec.containers)
        .filter(|container| {
            statuses.iter().any(|status| {
                status.name == container.name
                    && status
                        .state
                        .as_ref()
                        .is_some_and(|state| state.running.is_some())
            })
        })
        .map(|container| container.name.clone())
        .collect()
}

impl PodsPanel {
    /// The selected pod and its running containers - none when nothing is
    /// selected or the pod has no running container.
    pub(super) fn shell_candidates(&self, cx: &App) -> Option<(PodSelection, Vec<String>)> {
        let selection = self.table_selection(cx)?;
        let pod = self
            .table
            .read(cx)
            .find(&selection.namespace, &selection.name, None)?;
        let running = running_containers(pod);
        (!running.is_empty()).then_some((selection, running))
    }

    /// `ShellPod`: a shell in the selected pod's running container, or - with
    /// several - a choice of which first.
    pub(super) fn on_action_shell_pod(
        &mut self,
        _: &ShellPod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((selection, running)) = self.shell_candidates(cx) else {
            return;
        };
        if let [container] = running.as_slice() {
            open_shell(&selection, container, window, cx);
            return;
        }
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let mut footer = DialogFooter::new();
            for container in &running {
                let (selection, container) = (selection.clone(), container.clone());
                footer = footer.child(
                    Button::new(container_button_id(&container))
                        .label(container.clone())
                        .primary()
                        .on_click(move |_event, window, cx| {
                            window.close_dialog(cx);
                            open_shell(&selection, &container, window, cx);
                        }),
                );
            }
            dialog
                .title("Shell into Which Container?")
                .child(format!(
                    "Pod {} runs more than one container.",
                    selection.name
                ))
                .footer(
                    footer.child(Button::new(CANCEL_SHELL_ID).label("Cancel").on_click(
                        |_event, window, cx| {
                            window.close_dialog(cx);
                        },
                    )),
                )
        });
    }
}

fn open_shell(selection: &PodSelection, container: &str, window: &mut Window, cx: &mut App) {
    window.dispatch_action(
        Box::new(OpenExecSession {
            context_name: selection.context_name.clone(),
            target: ExecTarget {
                namespace: selection.namespace.clone(),
                pod: selection.name.clone(),
                container: container.to_string(),
            },
        }),
        cx,
    );
}

#[cfg(test)]
mod tests;
