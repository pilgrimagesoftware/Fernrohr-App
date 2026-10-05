//! Owns the pane's mutations: the auth choice, cancel, the delete confirmation
//! flow, save (section 1.2/4.2's validation), and section 4.3's connectivity test.

use super::*;

impl TunnelEditor {
    pub(super) fn set_auth(&mut self, auth: TunnelAuth, cx: &mut Context<Self>) {
        self.auth = auth;
        cx.notify();
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(TunnelEditorEvent::Cancelled);
    }

    pub(super) fn request_delete(&mut self, cx: &mut Context<Self>) {
        self.confirming_delete = true;
        cx.notify();
    }

    pub(super) fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.confirming_delete = false;
        cx.notify();
    }

    pub(super) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.editing_id.clone() else {
            return;
        };
        let store = TunnelStore::new(self.tunnels_path.clone());
        match store.delete(&id) {
            Ok(_unbound) => {
                self.confirming_delete = false;
                notify_tunnels_changed(cx);
                cx.emit(TunnelEditorEvent::Deleted);
            }
            Err(error) => {
                self.general_error = Some(format!("could not delete this tunnel: {error:?}"));
            }
        }
        cx.notify();
    }

    /// Section 1.2/4.2: validates and writes the tunnel through `TunnelStore`,
    /// generating a fresh id on first save. `TunnelStoreError::Invalid` is shown
    /// inline per field; every other error is shown once, generically.
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = TunnelStore::new(self.tunnels_path.clone());
        let name = self.name.read(cx).value().to_string();
        let host = self.host.read(cx).value().to_string();
        let user = self.user.read(cx).value().to_string();
        let port: u16 = self.port.read(cx).value().trim().parse().unwrap_or(0);
        let jump_hosts = split_jump_hosts(&self.jump_hosts.read(cx).value());
        let key_text = self.key_material.read(cx).value().to_string();
        let secret = (!key_text.is_empty()).then_some(key_text);

        // Fields this pane doesn't edit (another kind's settings) keep their stored
        // values.
        let existing = self
            .editing_id
            .as_deref()
            .and_then(|id| store.get(id))
            .unwrap_or_default();
        let tunnel = TunnelConfig {
            name,
            bastion_user: user,
            bastion_host: host,
            bastion_port: port,
            jump_hosts,
            auth: self.auth,
            ..existing
        };

        let result = match self.editing_id.clone() {
            Some(id) => store.update(&id, tunnel, secret.as_deref()),
            None => {
                let id = generate_tunnel_id();
                let result = store.create(&id, tunnel, secret.as_deref());
                if result.is_ok() {
                    self.editing_id = Some(id);
                }
                result
            }
        };

        match result {
            Ok(()) => {
                self.field_errors.clear();
                self.general_error = None;
                self.key_material
                    .update(cx, |state, cx| state.set_value("", window, cx));
                notify_tunnels_changed(cx);
                cx.emit(TunnelEditorEvent::Saved);
            }
            Err(TunnelStoreError::Invalid(errors)) => {
                self.field_errors = errors;
                self.general_error = None;
            }
            Err(other) => {
                self.field_errors.clear();
                self.general_error = Some(format!("could not save this tunnel: {other:?}"));
            }
        }
        cx.notify();
    }

    /// Section 4.3: runs the connectivity test against the pane's current (possibly
    /// unsaved) field values, on the tokio runtime, without ever touching
    /// `ForwardRegistry` - see `tunnel::ssh::test_connection`.
    pub(super) fn run_test(&mut self, cx: &mut Context<Self>) {
        let host = self.host.read(cx).value().to_string();
        let user = self.user.read(cx).value().to_string();
        let port: u16 = self.port.read(cx).value().trim().parse().unwrap_or(0);
        let jump_hosts = split_jump_hosts(&self.jump_hosts.read(cx).value());
        let key_text = self.key_material.read(cx).value().to_string();
        let id_for_identity = self
            .editing_id
            .clone()
            .unwrap_or_else(|| "unsaved-tunnel".to_string());
        let stored_secret = self.editing_id.as_deref().and_then(|id| {
            TunnelStore::new(self.tunnels_path.clone())
                .secret(id)
                .ok()
                .flatten()
        });
        let secret = if key_text.is_empty() {
            stored_secret
        } else {
            Some(key_text)
        };

        let identity_file = match secret
            .as_deref()
            .map(|secret| TransientIdentityFile::write(&id_for_identity, secret))
        {
            Some(Err(error)) => {
                self.test_result = Some(Err(format!("failed to prepare the test key: {error}")));
                cx.notify();
                return;
            }
            Some(Ok(file)) => Some(file),
            None => None,
        };

        let config = SshTunnelConfig {
            bastion_user: user,
            bastion_host: host,
            bastion_port: port,
            jump_hosts,
            remote_host: String::new(),
            remote_port: 0,
            local_port: 0,
            identity_file: identity_file.as_ref().map(TransientIdentityFile::path),
            known_hosts_file: None,
            ssh_config_file: None,
        };

        self.testing = true;
        self.test_result = None;
        cx.notify();

        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let result = test_connection(&config).await;
            let _ = tx.send(result).await;
            // Kept alive until the test finishes; Drop removes the file.
            drop(identity_file);
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, move |result| {
                let _ = this.update(cx, |editor, cx| {
                    editor.testing = false;
                    editor.test_result = Some(result);
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
