//! Closing a window that holds an unsaved edit (Fernrohr#168), through both of
//! its routes: the close button's should-close hook, and `Cmd-W`'s window
//! close (`close_whole_window`), with and without a tunnel the window would
//! also drop. Each asks irreversibly: Enter cancels, keeping the window and
//! the edit.

use super::tests::{Harness, edit_a_deployment, harness};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::tunnel;
use crate::tunnel::store::TunnelStore;
use crate::ui::nav::OpenedPanel;
use crate::util::shell::WindowMode;
use gpui_kit::{AppContext as _, TestAppContext};

impl Harness {
    /// Whether the window still shows the Deployment's unsaved edit.
    fn edit_kept(&mut self) -> bool {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                return false;
            };
            open_panels.iter().any(|open| match &open.panel {
                Some(OpenedPanel::ObjectDetail(panel)) => panel.read(cx).close_warning().is_some(),
                _ => false,
            })
        })
    }

    /// What `Cmd-W` does with no tab on screen: closes the window. Called
    /// directly - with any panel on screen, `Cmd-W` closes a tab instead, and
    /// a panel's edit is only ever open in one - so the guard is reached with
    /// an edit still open, as nothing in the app does yet.
    fn close_the_window(&mut self) {
        let main = self.main.clone();
        self.vcx
            .update(|window, cx| main.update(cx, |main, cx| main.close_whole_window(window, cx)));
        self.vcx.run_until_parked();
    }

    /// Binds the window's context, `demo`, to a tunnel with a live forward this
    /// window alone holds, so closing the window would drop it.
    fn hold_a_live_tunnel(&mut self) {
        let tunnels_path = crate::util::test_paths::temp_path("close-window-tunnels");
        let kubeconfig_path = crate::util::test_paths::temp_path("close-window-kubeconfig");
        std::fs::write(
            &kubeconfig_path,
            "apiVersion: v1\nkind: Config\nclusters:\n  - name: demo\n    cluster:\n      \
             server: https://10.0.0.1:6443\ncontexts:\n  - name: demo\n    context:\n      \
             cluster: demo\n      user: demo\nusers:\n  - name: demo\n    user: {}\n",
        )
        .expect("the kubeconfig fixture writes");
        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create(
                "test-bastion",
                crate::config::tunnels::TunnelConfig {
                    name: "test".into(),
                    bastion_user: "deploy".into(),
                    bastion_host: "bastion.example.com".into(),
                    bastion_port: 22,
                    ..Default::default()
                },
                None,
            )
            .expect("the tunnel saves");
        store.bind("demo", "test-bastion").expect("demo binds");
        let (_cluster, client) = crate::k8s::test_cluster::FakeCluster::start(&mut self.vcx.cx);
        self.vcx.update(|window, cx| {
            let forward =
                tunnel::acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "demo")
                    .expect("the tunnel resolves")
                    .expect("demo is bound to test-bastion");
            let connection = cx.new(|_| {
                ClusterConnection::test_with_state_and_forward(
                    ConnectionState::Connected(client),
                    forward,
                )
            });
            ClusterRegistry::insert_test_session_with_connection(cx, "demo", connection);
            let window_id = window.window_handle().window_id();
            if ClusterRegistry::holder_count(cx, "demo") == 0 {
                ClusterRegistry::hold(cx, "demo", window_id);
            }
        });
        self.vcx.run_until_parked();
        let main = self.main.clone();
        let tunneled = self
            .vcx
            .update(|_, cx| main.update(cx, |main, cx| main.contexts_losing_a_tunnel(cx)));
        assert_eq!(
            tunneled,
            ["demo"],
            "closing the window would drop demo's tunnel"
        );
    }
}

/// The close button: an unsaved edit keeps the window and asks - Enter
/// cancels, the deliberate shortcut closes it.
#[gpui_kit::test]
async fn the_close_button_with_an_unsaved_edit_asks_irreversibly(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    // The close button's hook, as `open_main_window` installs it.
    h.vcx.update(|window, cx| {
        window.on_window_should_close(cx, crate::util::shell::tabs::close_requested)
    });
    edit_a_deployment(&mut h);

    assert!(!h.vcx.simulate_close(), "an unsaved edit keeps the window");
    h.vcx.run_until_parked();
    assert!(h.dialog_open(), "and asks");
    h.first_frame();
    h.press("enter");
    assert!(!h.dialog_open(), "Enter closes the question");
    assert_eq!(h.vcx.windows().len(), 1, "and keeps the window");
    assert!(h.edit_kept(), "and the edit");

    assert!(!h.vcx.simulate_close());
    h.vcx.run_until_parked();
    h.first_frame();
    h.press("secondary-backspace");
    assert!(h.vcx.windows().is_empty(), "the shortcut closed the window");
}

/// `Cmd-W`'s window close over an unsaved edit asks, irreversibly: Enter keeps
/// the window and the edit.
#[gpui_kit::test]
async fn cmd_w_with_an_unsaved_edit_asks_irreversibly(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    edit_a_deployment(&mut h);

    h.close_the_window();
    assert!(h.dialog_open(), "an unsaved edit: it asks");
    h.first_frame();
    h.press("enter");
    assert!(!h.dialog_open(), "Enter closes the question");
    assert_eq!(h.vcx.windows().len(), 1, "and keeps the window");
    assert!(h.edit_kept(), "and the edit");

    h.close_the_window();
    h.first_frame();
    h.press("tab enter");
    assert!(
        h.vcx.windows().is_empty(),
        "Tab then Enter closed the window"
    );
}

/// The same with a tunnel the window would also drop: still irreversible, as
/// the edit decides - Enter keeps the window and the edit.
#[gpui_kit::test]
async fn cmd_w_with_an_unsaved_edit_and_a_tunnel_asks_irreversibly(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    h.hold_a_live_tunnel();
    edit_a_deployment(&mut h);

    h.close_the_window();
    assert!(h.dialog_open(), "an unsaved edit and a tunnel: it asks");
    h.first_frame();
    h.press("enter");
    assert!(!h.dialog_open(), "Enter closes the question");
    assert_eq!(h.vcx.windows().len(), 1, "and keeps the window");
    assert!(h.edit_kept(), "and the edit");

    h.close_the_window();
    h.first_frame();
    h.press("secondary-backspace");
    assert!(h.vcx.windows().is_empty(), "the shortcut closed the window");
}
