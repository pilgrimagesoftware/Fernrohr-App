// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::command::CommandRegistry;
use crate::config::tunnels::{TunnelAuth, TunnelConfig};
use crate::util::shell::test_support::*;
use crate::util::shell::{SET_CONTEXT_TUNNEL_COMMAND_ID, register_commands, write_context_tunnel};

/// Tasks.md 3.2: `context.set_tunnel` is registered with a title, alongside every
/// other palette command.
#[test]
fn set_context_tunnel_is_a_registered_command() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);

    let command = registry
        .get(SET_CONTEXT_TUNNEL_COMMAND_ID)
        .expect("context.set_tunnel must be registered");
    assert_eq!(command.title, "Set Tunnel for Context");
}

/// Tasks.md 3.2: the command's handler binds - `write_context_tunnel` is the
/// write `on_action_set_tunnel`'s dialog options call, factored out so it's
/// testable without a `Root` (which its `open_dialog`/`close_dialog` calls
/// require).
#[test]
fn set_context_tunnels_handler_binds_and_unbinds() {
    let path = temp_tunnels_path();
    let store = crate::tunnel::store::TunnelStore::new(path.clone());
    store
        .create(
            "qa-bastion",
            TunnelConfig {
                name: "QA".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
            },
            None,
        )
        .unwrap();

    write_context_tunnel(&path, "qa-1", Some("qa-bastion")).unwrap();
    assert_eq!(store.binding_for("qa-1"), Some("qa-bastion".to_string()));

    write_context_tunnel(&path, "qa-1", None).unwrap();
    assert_eq!(store.binding_for("qa-1"), None);

    let _ = std::fs::remove_file(&path);
}
