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

        let command_line = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "gcloud compute ssh <host> --tunnel-through-iap -- -N -L{port}:127.0.0.1:8888",
            )
        });
        let local_port =
            cx.new(|cx| InputState::new(window, cx).placeholder("Allocated when empty"));
        let startup_timeout = cx.new(|cx| InputState::new(window, cx).placeholder("30"));

        let auth = existing
            .as_ref()
            .map(|tunnel| tunnel.auth)
            .unwrap_or_default();
        let kind = existing
            .as_ref()
            .map(|tunnel| tunnel.kind)
            .unwrap_or_default();
        let command = existing
            .as_ref()
            .map(|tunnel| tunnel.command.clone())
            .unwrap_or_default();
        command_line.update(cx, |state, cx| {
            state.set_value(command.command_line.clone(), window, cx)
        });
        local_port.update(cx, |state, cx| {
            let port = command.local_port.map(|port| port.to_string());
            state.set_value(port.unwrap_or_default(), window, cx)
        });
        startup_timeout.update(cx, |state, cx| {
            state.set_value(command.startup_timeout_secs.to_string(), window, cx)
        });

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
            kind,
            host,
            user,
            port,
            jump_hosts,
            auth,
            key_material,
            command_line,
            mode: command.mode,
            local_port,
            startup_timeout,
            field_errors: Vec::new(),
            general_error: None,
            bound_contexts,
            testing: false,
            test_result: None,
            focus_handle: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            sections: Vec::new(),
            revealed: None,
        }
    }
}
