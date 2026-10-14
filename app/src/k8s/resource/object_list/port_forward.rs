//! Port-forward the selected Service (`k9s-remaining-keybindings` 4.1, 4.2):
//! in a Services list, `shift-f` forwards one of the Service's ports - asking
//! which when it has several - to a Running Pod behind it, into the app's
//! forwards that Manage Tunnels lists and stops.
//!
//! Offered only in a list of core Services: the table sits in a
//! [`SERVICES_KEY_CONTEXT`] element there, which is the command's context.

use super::commands::PortForwardService;
use super::panel::ObjectListPanel;
use crate::k8s::cluster::port_forwards::ForwardObject;
use crate::k8s::resource::port_forwarding;
use gpui_kit::*;

/// The context the Services port-forward command lives in.
pub const SERVICES_KEY_CONTEXT: &str = "ServicesList";
impl ObjectListPanel {
    /// Whether this panel lists core Services.
    pub(super) fn lists_services(&self) -> bool {
        self.kind.gvk.group.is_empty() && self.kind.gvk.kind == "Service"
    }

    /// The selected Service's forwards; none in any other list.
    pub(super) fn selected_forwards(
        &self,
        cx: &App,
    ) -> Vec<crate::k8s::cluster::port_forwards::ForwardSummary> {
        let Some(row_ix) = self.selected_row(cx) else {
            return Vec::new();
        };
        let Some(row) = self
            .table
            .as_ref()
            .and_then(|table| table.read(cx).delegate().rows().get(row_ix).cloned())
        else {
            return Vec::new();
        };
        self.forwards_of(&row.object, cx)
    }

    /// `StopPortForward` in a Services list: stops one of the selected Service's
    /// forwards - asking which when it has several - once confirmed.
    pub(super) fn on_action_stop_port_forward(
        &mut self,
        _: &crate::k8s::resource::pods::StopPortForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, name)) = self.selected_object(cx) else {
            return;
        };
        let forwards = self.selected_forwards(cx);
        crate::ui::forward_stop::stop_one_of("service", name, forwards, window, cx);
    }

    /// The forwards started from `row`'s Service; none in any other list.
    pub(super) fn forwards_of(
        &self,
        row: &super::row::ObjectRow,
        cx: &App,
    ) -> Vec<crate::k8s::cluster::port_forwards::ForwardSummary> {
        let (true, Some(namespace), Some(forwards)) = (
            self.lists_services(),
            row.namespace.as_deref(),
            crate::k8s::cluster::port_forwards::PortForwards::existing(cx),
        ) else {
            return Vec::new();
        };
        let service = ForwardObject::service(&self.scope.context_name, namespace, &row.name);
        forwards.read(cx).for_object(&service)
    }

    /// The selected row's namespace and name.
    fn selected_object(&self, cx: &App) -> Option<(String, String)> {
        let row_ix = self.selected_row(cx)?;
        let row = self
            .table
            .as_ref()?
            .read(cx)
            .delegate()
            .rows()
            .get(row_ix)?
            .clone();
        Some((row.object.namespace.clone()?, row.object.name.clone()))
    }

    /// `PortForwardService`: forwards the selected Service
    /// ([`port_forwarding::forward_service`]).
    pub(super) fn on_action_port_forward_service(
        &mut self,
        _: &PortForwardService,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.lists_services() {
            return;
        }
        let Some((namespace, name)) = self.selected_object(cx) else {
            return;
        };
        port_forwarding::forward_service(
            self.scope.context_name.clone(),
            &self.connection,
            namespace,
            name,
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests;
