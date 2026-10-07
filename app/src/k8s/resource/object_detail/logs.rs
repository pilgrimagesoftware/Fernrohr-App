//! The logs of the pods an object selects (#150): `l` in a Deployment's - or
//! any workload's or Service's - detail panel opens a Logs panel following
//! every pod its `spec.selector` picks. The panel itself is `util::logs`'.

use super::commands::{POD_LOGS_KEY, POD_SELECTING_KEY_CONTEXT, ShowObjectLogs};
use super::panel::ObjectDetailPanel;
use crate::k8s::label_selector;
use crate::util::logs::{OpenWorkloadLogs, WorkloadRef};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// The header's Logs hint, for tests.
pub(super) const LOGS_HINT: &str = "object-logs-hint";

impl ObjectDetailPanel {
    /// The workload whose pods `l` follows: the object as loaded, when it is
    /// namespaced and its spec selects pods.
    pub(super) fn pod_logs_workload(&self) -> Option<WorkloadRef> {
        let namespace = self.target.namespace.clone()?;
        let selector = label_selector::of_object(self.object()?)?;
        Some(WorkloadRef {
            kind: self.target.kind.gvk.kind.clone(),
            namespace,
            name: self.target.name.clone(),
            selector: selector.to_string(),
        })
    }

    /// `ShowObjectLogs`: opens the Logs panel following the object's pods.
    pub(super) fn on_action_show_logs(
        &mut self,
        _: &ShowObjectLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workload) = self.pod_logs_workload() {
            window.dispatch_action(
                Box::new(OpenWorkloadLogs {
                    context_name: self.scope.context_name.clone(),
                    workload,
                }),
                cx,
            );
        }
    }

    /// The header's Logs hint - its key from the live keymap, and a click
    /// doing what the key does - while the object selects pods.
    pub(super) fn logs_hint(&self, window: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        self.pod_logs_workload()?;
        let key = Kbd::binding_for_action(&ShowObjectLogs, Some(POD_SELECTING_KEY_CONTEXT), window)
            .unwrap_or_else(|| Kbd::new(Keystroke::parse(POD_LOGS_KEY).expect("valid keybinding")));
        Some(
            div()
                .id(LOGS_HINT)
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .whitespace_nowrap()
                .cursor_pointer()
                .child(key)
                .child("Logs")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.on_action_show_logs(&ShowObjectLogs, window, cx);
                }))
                .into_any_element(),
        )
    }
}
