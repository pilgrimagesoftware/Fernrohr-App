//! Revealing a Secret's values one at a time in the object viewer - the same
//! rule, and the same `SecretValue`, as pod detail's Configuration tab.

use super::commands::HideSecretValues;
use super::panel::ObjectDetailPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::secret_value::{Reveal, reveal};
use gpui_kit::*;

impl ObjectDetailPanel {
    /// Reveals `key` of `secret` with a fresh read of that one value, or hides
    /// it if it's already revealed. A reveal that lands after its value was
    /// hidden again is dropped.
    pub(super) fn toggle_reveal(&mut self, secret: ObjectRef, key: String, cx: &mut Context<Self>) {
        if self.revealed.remove(&key).is_some() {
            cx.notify();
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        self.revealed.insert(key.clone(), Reveal::Pending);
        let namespace = secret.namespace.unwrap_or_default();
        let fetched = key.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx
                .send(reveal(client, namespace, secret.name, fetched).await)
                .await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    if let Some(entry) = this.revealed.get_mut(&key)
                        && matches!(entry, Reveal::Pending)
                    {
                        *entry = match result {
                            Ok(value) => Reveal::Shown(value),
                            Err(error) => Reveal::Failed(error),
                        };
                        cx.notify();
                    }
                });
            })
            .await;
        })
        .detach();
        cx.notify();
    }

    pub(super) fn on_action_hide_secret_values(
        &mut self,
        _: &HideSecretValues,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.revealed.clear();
        cx.notify();
    }
}
