//! The list's Uninstall (helm-releases section 8): on the selected row, by
//! its key, the palette or the row's menu, counting the deployed revision's
//! objects for the confirmation. Confirming and running are
//! `helm::actions`'.

use super::panel::HelmReleasesPanel;
use crate::helm_cli;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::helm::actions::{self, ActionHost, ActionState};
use gpui_kit::*;

impl ActionHost for HelmReleasesPanel {
    fn action_state(&mut self) -> &mut ActionState {
        &mut self.actions
    }
}

impl HelmReleasesPanel {
    /// Why Uninstall can't run now, if it can't.
    pub(super) fn actions_unavailable(&self, cx: &App) -> Option<String> {
        helm_cli::availability(&self.scope.context_name, cx)
            .err()
            .map(|why| why.to_string())
    }

    /// Tells the palette when Uninstall can't run from the list, and why
    /// (#406): `helm` or the connection isn't usable, or no release is selected.
    pub(super) fn register_unavailable_reasons(&self, cx: &mut Context<Self>) {
        let panel = cx.weak_entity();
        crate::command::unavailable::register(
            &self.focus_handle,
            move |action, cx| {
                if !action
                    .as_any()
                    .is::<crate::k8s::resource::helm::actions::UninstallRelease>()
                {
                    return None;
                }
                let panel = panel.upgrade()?;
                let panel = panel.read(cx);
                if let Some(why) = panel.actions_unavailable(cx) {
                    return Some(why.into());
                }
                let selected = panel
                    .table
                    .as_ref()
                    .is_some_and(|table| table.read(cx).delegate().selected().is_some());
                (!selected).then(|| "No release is selected.".into())
            },
            cx,
        );
    }

    /// Uninstall's command, key and menu item: confirms uninstalling the
    /// selected release. Nothing selected, nothing happens.
    pub(super) fn uninstall_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self
            .table
            .as_ref()
            .and_then(|table| table.read(cx).delegate().selected().cloned())
        else {
            return;
        };
        let key = crate::k8s::resource::helm::ReleaseKey {
            namespace: row.namespace.clone(),
            name: row.name.clone(),
        };
        let deployed = self.tables.iter().find_map(|watched| {
            let history = watched.table.read(cx).history(&key);
            crate::k8s::resource::helm::current_revision(&history)
                .map(|revision| revision.secret.clone())
        });
        let client = match &self.connection.read(cx).state {
            ConnectionState::Connected(client) => Some(client.clone()),
            _ => None,
        };
        let release = helm_cli::ReleaseKey {
            context: self.scope.context_name.clone(),
            namespace: row.namespace,
            release: row.name,
        };
        actions::confirm_uninstall(self, release, client.zip(deployed), window, cx);
    }
}

impl HelmReleasesPanel {
    /// The selected release's name. Test-only.
    #[cfg(test)]
    pub(crate) fn selected_release(&self, cx: &App) -> Option<String> {
        self.table.as_ref().and_then(|table| {
            table
                .read(cx)
                .delegate()
                .selected()
                .map(|row| row.name.clone())
        })
    }
}
