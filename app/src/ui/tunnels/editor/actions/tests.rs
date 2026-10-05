// NAMED imports only (no `use super::*;`): a glob of `gpui_kit::*` next to
// `#[gpui_kit::test]` shadows the builtin `#[test]` and blows the macro-expansion budget.
use crate::config::tunnels::{TunnelAuth, TunnelsConfig};
use crate::tunnel::store::{TunnelFieldError, TunnelStore};
use crate::ui::tunnels::editor::TunnelEditor;
use gpui_kit::TestAppContext;
use std::path::PathBuf;

fn temp_tunnels_path() -> PathBuf {
    crate::util::test_paths::temp_path("tunnel-editor")
}

fn config_at(path: &std::path::Path) -> TunnelsConfig {
    crate::config::load(path)
}

/// Tasks.md 4.2: creating a tunnel writes it to `tunnels.toml` under a fresh id.
#[gpui_kit::test]
async fn creating_a_tunnel_saves_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path = temp_tunnels_path();
    let window = cx.add_window({
        let path = path.clone();
        move |window, cx| TunnelEditor::create(path, window, cx)
    });

    window
        .update(cx, |editor, window, cx| {
            editor
                .name
                .update(cx, |s, cx| s.set_value("QA Bastion", window, cx));
            editor
                .host
                .update(cx, |s, cx| s.set_value("bastion.example.com", window, cx));
            editor
                .user
                .update(cx, |s, cx| s.set_value("ops", window, cx));
            editor.save(window, cx);
        })
        .unwrap();

    let saved = config_at(&path);
    assert_eq!(saved.tunnels.len(), 1);
    let (_, tunnel) = saved.tunnels.into_iter().next().unwrap();
    assert_eq!(tunnel.name, "QA Bastion");
    assert_eq!(tunnel.bastion_host, "bastion.example.com");
    assert_eq!(tunnel.bastion_user, "ops");
    assert_eq!(
        tunnel.bastion_port, 22,
        "default port carries through unedited"
    );

    let _ = std::fs::remove_file(&path);
}

/// Tasks.md 4.2: renaming an existing tunnel keeps its id (and so its bindings)
/// intact - only `name` changes underneath a stable key.
#[gpui_kit::test]
async fn renaming_keeps_bindings_intact(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path = temp_tunnels_path();
    let store = TunnelStore::new(path.clone());
    store
        .create(
            "qa-bastion",
            crate::config::tunnels::TunnelConfig {
                name: "Old Name".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    store.bind("qa-1", "qa-bastion").unwrap();

    let window = cx.add_window({
        let path = path.clone();
        move |window, cx| TunnelEditor::edit(path, "qa-bastion".to_string(), window, cx)
    });

    window
        .update(cx, |editor, window, cx| {
            editor
                .name
                .update(cx, |s, cx| s.set_value("New Name", window, cx));
            editor.save(window, cx);
        })
        .unwrap();

    assert_eq!(
        store.bindings(),
        vec![("qa-1".to_string(), "qa-bastion".to_string())],
        "the binding must still point at the same tunnel id after a rename"
    );
    assert_eq!(store.get("qa-bastion").unwrap().name, "New Name");

    let _ = std::fs::remove_file(&path);
}

/// Tasks.md 4.2: an invalid port is rejected and named inline, and nothing is
/// written.
#[gpui_kit::test]
async fn an_invalid_port_is_rejected(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path = temp_tunnels_path();
    let window = cx.add_window({
        let path = path.clone();
        move |window, cx| TunnelEditor::create(path, window, cx)
    });

    window
        .update(cx, |editor, window, cx| {
            editor
                .name
                .update(cx, |s, cx| s.set_value("Bad Port", window, cx));
            editor
                .host
                .update(cx, |s, cx| s.set_value("bastion.example.com", window, cx));
            editor
                .user
                .update(cx, |s, cx| s.set_value("ops", window, cx));
            editor
                .port
                .update(cx, |s, cx| s.set_value("not-a-port", window, cx));
            editor.save(window, cx);
        })
        .unwrap();

    window
        .update(cx, |editor, _window, _cx| {
            assert_eq!(editor.field_errors, vec![TunnelFieldError::InvalidPort]);
        })
        .unwrap();
    assert!(
        config_at(&path).tunnels.is_empty(),
        "an invalid tunnel must not save"
    );

    let _ = std::fs::remove_file(&path);
}

/// Tasks.md 4.2: the delete confirmation names every context bound to the tunnel
/// being deleted.
#[gpui_kit::test]
async fn delete_confirm_names_the_bound_contexts(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path = temp_tunnels_path();
    let store = TunnelStore::new(path.clone());
    store
        .create(
            "qa-bastion",
            crate::config::tunnels::TunnelConfig {
                name: "QA".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    store.bind("qa-1", "qa-bastion").unwrap();
    store.bind("qa-2", "qa-bastion").unwrap();

    let window = cx.add_window({
        let path = path.clone();
        move |window, cx| TunnelEditor::edit(path, "qa-bastion".to_string(), window, cx)
    });

    window
        .update(cx, |editor, _window, _cx| {
            assert_eq!(
                editor.bound_contexts,
                vec!["qa-1".to_string(), "qa-2".to_string()]
            );
            editor.request_delete(_cx);
            assert!(editor.confirming_delete);
        })
        .unwrap();

    let _ = std::fs::remove_file(&path);
}
