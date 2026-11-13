//! The detail panel's Rollback and Uninstall (helm-releases section 8): which
//! revision Rollback goes to - History's selection, or the one before the
//! current revision when that is what's selected - and which revision's
//! manifest Uninstall counts. Confirming and running are `helm::actions`'.

use super::commands::PANEL_KEY_CONTEXT;
use super::panel::HelmReleasePanel;
use crate::helm_cli;
use crate::k8s::resource::helm::actions::{
    self, ActionHost, ActionState, RevisionLine, Rollback, RollbackRelease, UninstallRelease,
};
use gpui_kit::assets::IconName;
use gpui_kit::*;

/// The header's action buttons.
pub(crate) const ROLLBACK_BUTTON: &str = "helm-release-rollback";
pub(crate) const UNINSTALL_BUTTON: &str = "helm-release-uninstall";

impl ActionHost for HelmReleasePanel {
    fn action_state(&mut self) -> &mut ActionState {
        &mut self.actions
    }
}

impl HelmReleasePanel {
    /// The release as `helm` names it.
    pub(super) fn run_key(&self) -> helm_cli::ReleaseKey {
        helm_cli::ReleaseKey {
            context: self.scope.context_name.clone(),
            namespace: self.key.namespace.clone(),
            release: self.key.name.clone(),
        }
    }

    /// Why Rollback and Uninstall can't run now, if they can't.
    pub(super) fn actions_unavailable(&self, cx: &App) -> Option<String> {
        helm_cli::availability(&self.scope.context_name, cx)
            .err()
            .map(|why| why.to_string())
    }

    /// Where Rollback would go: History's selection, unless that is the
    /// current revision, in which case the one before it.
    pub(crate) fn rollback_target(&self, cx: &App) -> Option<u32> {
        let history = self.history(cx);
        let current = history.first()?.version;
        match self.selected_revision(cx) {
            Some(selected) if selected != current => Some(selected),
            _ => history.get(1).map(|revision| revision.version),
        }
    }

    fn revision_line(&self, version: u32) -> RevisionLine {
        let detail = self.detail(version);
        RevisionLine {
            version,
            chart: detail.map(|detail| format!("{}-{}", detail.chart.name, detail.chart.version)),
            app_version: detail.and_then(|detail| detail.chart.app_version.clone()),
        }
    }

    /// Rollback's command, key and button: confirms rolling back to
    /// [`Self::rollback_target`].
    pub(super) fn rollback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let what = format!("Roll back {}", self.key.name);
        let current = self.history(cx).first().map(|revision| revision.version);
        let (Some(current), Some(target)) = (current, self.rollback_target(cx)) else {
            actions::refuse(
                self,
                &what,
                "There's no earlier revision to roll back to.".into(),
                cx,
            );
            return;
        };
        let rollback = Rollback {
            release: self.run_key(),
            current: self.revision_line(current),
            target: self.revision_line(target),
        };
        actions::confirm_rollback(self, rollback, window, cx);
    }

    /// Uninstall's command, key and button: confirms uninstalling, counting
    /// the deployed revision's objects.
    pub(super) fn uninstall(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let history = self.history(cx);
        let deployed = history
            .iter()
            .find(|revision| revision.status == "deployed")
            .or(history.first())
            .map(|revision| revision.secret.clone());
        let deployed = self.client(cx).zip(deployed);
        actions::confirm_uninstall(self, self.run_key(), deployed, window, cx);
    }
}

impl HelmReleasePanel {
    /// Rollback's and Uninstall's icon buttons, once the release is listed:
    /// disabled, saying why, while `helm` or the connection isn't available.
    pub(super) fn action_buttons(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.history(cx).is_empty() {
            return Vec::new();
        }
        let unavailable = self.actions_unavailable(cx);
        let panel = cx.weak_entity();
        let rollback = {
            let panel = panel.clone();
            actions::action_button(
                ROLLBACK_BUTTON,
                IconName::Undo2,
                "Roll back",
                &RollbackRelease,
                PANEL_KEY_CONTEXT,
                unavailable.clone(),
                move |window, cx| {
                    let _ = panel.update(cx, |this, cx| this.rollback(window, cx));
                },
            )
        };
        let uninstall = actions::action_button(
            UNINSTALL_BUTTON,
            IconName::Trash,
            "Uninstall",
            &UninstallRelease,
            PANEL_KEY_CONTEXT,
            unavailable,
            move |window, cx| {
                let _ = panel.update(cx, |this, cx| this.uninstall(window, cx));
            },
        );
        vec![rollback, uninstall]
    }
}
