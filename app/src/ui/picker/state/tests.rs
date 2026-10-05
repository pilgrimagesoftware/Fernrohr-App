// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
use super::{Attempt, ClusterPicker};
use crate::k8s::cluster::kubeconfig;
use std::fs;

const FIXTURE: &str = r#"
apiVersion: v1
kind: Config
clusters:
  - name: kind-dev
    cluster:
      server: https://127.0.0.1:6443
contexts:
  - name: kind-dev
    context:
      cluster: kind-dev
      user: kind-dev
current-context: kind-dev
users:
  - name: kind-dev
    user: {}
"#;

fn fixture_path() -> std::path::PathBuf {
    let path = crate::util::test_paths::temp_path("picker-fixture").with_extension("yaml");
    fs::write(&path, FIXTURE).unwrap();
    path
}

#[test]
fn lists_contexts_from_a_valid_kubeconfig() {
    let path = fixture_path();
    let contexts = kubeconfig::list_context_names(Some(&path)).unwrap();
    assert_eq!(contexts, vec!["kind-dev".to_string()]);
}

#[test]
fn missing_kubeconfig_is_reported_as_an_error() {
    let missing = std::env::temp_dir().join("fernrohr-picker-fixture-does-not-exist.yaml");
    let result = kubeconfig::list_context_names(Some(&missing));
    assert!(result.is_err());
}

/// Section 3.2: drives `ClusterPicker` into a fake `Failed` attempt directly (via
/// `ClusterConnection::test_with_state`, no real connect) rather than through
/// `select`, so the failure path doesn't depend on network access or a real
/// kubeconfig - then confirms the picker remains interactive by driving a second
/// `select` call afterward.
///
/// The retry also goes through the stub connection factory rather than
/// `ClusterRegistry::connection`: a real connect spawns tokio work on a runtime
/// worker thread, which gpui's test scheduler flags as nondeterminism and turns
/// into a flaky failure. This test previously failed that way on `develop`.
#[gpui_kit::test]
async fn failed_attempt_shows_the_reason_and_stays_interactive(cx: &mut gpui_kit::TestAppContext) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use gpui_kit::{AppContext as _, Entity};

    /// Stands in for `ClusterRegistry::connection`: hands back a connection in a
    /// non-terminal state so selecting again replaces the attempt without any
    /// real I/O.
    fn stub_connection(cx: &mut gpui_kit::App, _context_name: &str) -> Entity<ClusterConnection> {
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting))
    }

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);

    window
        .update(cx, |picker, _window, cx| {
            picker.connection_factory = Some(stub_connection);
            picker.attempt = Some(Attempt {
                context_name: "kind-dev".to_string(),
                connection: cx.new(|_| {
                    ClusterConnection::test_with_state(ConnectionState::Failed(
                        "connection refused".to_string(),
                    ))
                }),
                connected: false,
            });
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            let attempt = picker.attempt.as_ref().expect("attempt is still set");
            assert_eq!(attempt.context_name, "kind-dev");
            assert!(matches!(
                attempt.connection.read(cx).state,
                ConnectionState::Failed(ref reason) if reason == "connection refused"
            ));
        })
        .unwrap();

    // Stays interactive: a failed attempt doesn't leave the picker stuck - selecting
    // again (retry, or a different context) starts a fresh attempt.
    window
        .update(cx, |picker, _window, cx| {
            picker.select("kind-dev".to_string(), cx)
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker
                    .attempt
                    .as_ref()
                    .expect("select started a new attempt")
                    .context_name,
                "kind-dev"
            );
        })
        .unwrap();
}

/// Tasks.md 3.1: selecting a tunnel for a row writes through `TunnelStore::bind`
/// (asserted at the file, since `tunnels.toml` is the only state) and updates the
/// cached binding the row's label reads (`context_row`/`picker_tunnel::bound_label`
/// turn that into text - covered on their own in `picker_tunnel.rs`'s tests);
/// selecting Direct (`None`) unbinds it again.
#[gpui_kit::test]
async fn set_tunnel_binds_and_unbinds_through_the_store(cx: &mut gpui_kit::TestAppContext) {
    use crate::config::tunnels::TunnelAuth;
    use crate::tunnel::store::TunnelStore;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let tunnels_path = std::env::temp_dir().join(format!(
        "fernrohr-picker-set-tunnel-test-{}.toml",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tunnels_path);
    let store = TunnelStore::new(tunnels_path.clone());
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

    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            // Test-only override: a fresh `ClusterPicker` otherwise reads this
            // machine's real `tunnels.toml`, which this test must not touch.
            picker.tunnels_path = tunnels_path.clone();
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            picker.set_tunnel("kind-dev", Some("qa-bastion".to_string()), cx);
        })
        .unwrap();
    assert_eq!(
        TunnelStore::new(tunnels_path.clone()).binding_for("kind-dev"),
        Some("qa-bastion".to_string())
    );
    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker.tunnel_bindings.get("kind-dev"),
                Some(&"qa-bastion".to_string()),
                "the cached binding a row's label reads must update"
            );
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            picker.set_tunnel("kind-dev", None, cx);
        })
        .unwrap();
    assert_eq!(
        TunnelStore::new(tunnels_path.clone()).binding_for("kind-dev"),
        None,
        "selecting Direct must remove the binding"
    );
    window
        .update(cx, |picker, _window, _cx| {
            assert!(!picker.tunnel_bindings.contains_key("kind-dev"));
        })
        .unwrap();

    let _ = std::fs::remove_file(&tunnels_path);
}

/// A tunnel created elsewhere (the Tunnels window) reaches an already-open picker's
/// row dropdown once the write bumps `TunnelsRevision`, without the picker writing
/// anything itself.
#[gpui_kit::test]
async fn a_tunnel_created_elsewhere_appears_in_an_open_picker(cx: &mut gpui_kit::TestAppContext) {
    use crate::config::tunnels::{TunnelAuth, TunnelConfig};
    use crate::tunnel::store::TunnelStore;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let tunnels_path = std::env::temp_dir().join(format!(
        "fernrohr-picker-revision-test-{}.toml",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tunnels_path);

    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            // Test-only override, as above: never read this machine's real file.
            picker.tunnels_path = tunnels_path.clone();
            picker.tunnel_choices.clear();
        })
        .unwrap();

    TunnelStore::new(tunnels_path.clone())
        .create(
            "qa-bastion",
            TunnelConfig {
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
    cx.update(crate::ui::tunnels::notify_tunnels_changed);
    cx.run_until_parked();

    window
        .update(cx, |picker, _window, _cx| {
            assert!(
                picker
                    .tunnel_choices
                    .iter()
                    .any(|choice| choice.id == "qa-bastion"),
                "the new tunnel should be offered without reopening the picker"
            );
        })
        .unwrap();
    let _ = std::fs::remove_file(&tunnels_path);
}

/// `window-context-bar` section 3.2: the "+" popover's own filtering -
/// `StatusBarView::open_add_dialog` (`ui/status_bar/capsule.rs`) calls this with the
/// window's current `contexts` so the popover never re-offers a context already
/// in use. Setting `contexts` directly (rather than through `ClusterPicker::new`,
/// which reads this machine's real kubeconfig) keeps the candidate list under
/// this test's own control.
#[gpui_kit::test]
async fn exclude_removes_used_contexts_and_leaves_the_rest(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            picker.contexts = Ok(vec![
                "kind-dev".to_string(),
                "staging".to_string(),
                "prod".to_string(),
            ]);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            picker.exclude(&["kind-dev".to_string(), "staging".to_string()]);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker.contexts.as_deref(),
                Ok(["prod".to_string()].as_slice()),
                "only the unused context remains"
            );
        })
        .unwrap();
}

/// Excluding a context this picker never listed (a window using a context this
/// kubeconfig no longer has) is a no-op, not a panic or a silent truncation.
#[gpui_kit::test]
async fn excluding_an_unlisted_context_changes_nothing(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            picker.contexts = Ok(vec!["kind-dev".to_string()]);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            picker.exclude(&["never-listed".to_string()]);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker.contexts.as_deref(),
                Ok(["kind-dev".to_string()].as_slice())
            );
        })
        .unwrap();
}

/// A picker whose kubeconfig read already failed has nothing to filter - `exclude`
/// must leave the error alone rather than panicking on the `Err` case.
#[gpui_kit::test]
async fn excluding_from_a_failed_picker_leaves_the_error_alone(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            picker.contexts = Err("missing kubeconfig".to_string());
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            picker.exclude(&["kind-dev".to_string()]);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(picker.contexts, Err("missing kubeconfig".to_string()));
        })
        .unwrap();
}
