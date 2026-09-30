//! Owns building a fresh or existing-tunnel pane: reading the stored tunnel (if any)
//! and its bound contexts, and seeding each input's starting value.

use super::*;

impl TunnelEditor {
    pub fn create(tunnels_path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(tunnels_path, None, window, cx)
    }

    pub fn edit(
        tunnels_path: PathBuf,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(tunnels_path, Some(id), window, cx)
    }

    fn new(
        tunnels_path: PathBuf,
        editing_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = TunnelStore::new(tunnels_path.clone());
        let existing = editing_id.as_deref().and_then(|id| store.get(id));
        let bound_contexts: Vec<String> = editing_id
            .as_deref()
            .map(|id| {
                let mut contexts: Vec<String> = store
                    .bindings()
                    .into_iter()
                    .filter(|(_, tunnel_id)| tunnel_id == id)
                    .map(|(context, _)| context)
                    .collect();
                contexts.sort();
                contexts
            })
            .unwrap_or_default();

        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let host = cx.new(|cx| {
            InputState::new(window, cx).placeholder("bastion.example.com, or an ssh config alias")
        });
        let user = cx.new(|cx| InputState::new(window, cx).placeholder("User"));
        let port = cx.new(|cx| InputState::new(window, cx).placeholder("22"));
        let jump_hosts = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("user@hop1, user@hop2 (optional, nearest-to-target last)")
        });
        let key_material = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Private key")
        });

        let auth = existing
            .as_ref()
            .map(|tunnel| tunnel.auth)
            .unwrap_or_default();

        if let Some(tunnel) = &existing {
            name.update(cx, |state, cx| {
                state.set_value(tunnel.name.clone(), window, cx)
            });
            host.update(cx, |state, cx| {
                state.set_value(tunnel.bastion_host.clone(), window, cx)
            });
            user.update(cx, |state, cx| {
                state.set_value(tunnel.bastion_user.clone(), window, cx)
            });
            port.update(cx, |state, cx| {
                state.set_value(tunnel.bastion_port.to_string(), window, cx)
            });
            jump_hosts.update(cx, |state, cx| {
                state.set_value(tunnel.jump_hosts.join(", "), window, cx)
            });
        } else {
            port.update(cx, |state, cx| state.set_value("22", window, cx));
        }

        Self {
            tunnels_path,
            editing_id,
            name,
            host,
            user,
            port,
            jump_hosts,
            auth,
            key_material,
            field_errors: Vec::new(),
            general_error: None,
            bound_contexts,
            confirming_delete: false,
            testing: false,
            test_result: None,
            focus_handle: cx.focus_handle(),
        }
    }
}
