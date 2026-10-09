use super::*;
use crate::config::tunnels::TunnelConfig;
use crate::tunnel::store::TunnelStore;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_tunnels_path() -> std::path::PathBuf {
    crate::util::test_paths::temp_path("cluster-tunnel")
}

/// Writes a throwaway kubeconfig whose contexts map 1:1 to `(context, server)`
/// pairs, e.g. `[("staging", "https://10.0.0.1:6443")]`.
fn kubeconfig_fixture(contexts: &[(&str, &str)]) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "fernrohr-cluster-tunnel-kubeconfig-{}-{n}.yaml",
        std::process::id()
    ));

    let mut clusters = String::new();
    let mut ctxs = String::new();
    let mut users = String::new();
    for (name, server) in contexts {
        clusters.push_str(&format!(
            "  - name: {name}\n    cluster:\n      server: {server}\n"
        ));
        ctxs.push_str(&format!(
            "  - name: {name}\n    context:\n      cluster: {name}\n      user: {name}\n"
        ));
        users.push_str(&format!("  - name: {name}\n    user: {{}}\n"));
    }
    let yaml = format!(
        "apiVersion: v1\nkind: Config\nclusters:\n{clusters}contexts:\n{ctxs}users:\n{users}"
    );
    std::fs::write(&path, yaml).unwrap();
    path
}

fn sample_tunnel(name: &str) -> TunnelConfig {
    TunnelConfig {
        name: name.to_string(),
        bastion_user: "deploy".to_string(),
        bastion_host: "bastion.example.invalid".to_string(),
        bastion_port: 22,
        jump_hosts: Vec::new(),
        auth: crate::config::tunnels::TunnelAuth::default(),
        ..Default::default()
    }
}

#[test]
fn store_and_io_errors_convert_into_a_tunnel_acquire_error() {
    assert!(matches!(
        TunnelAcquireError::from(TunnelStoreError::NotFound),
        TunnelAcquireError::Store(TunnelStoreError::NotFound)
    ));
    let io_err = io::Error::from(io::ErrorKind::AddrInUse);
    assert!(matches!(
        TunnelAcquireError::from(io_err),
        TunnelAcquireError::Io(_)
    ));
}

#[gpui_kit::test]
async fn unbound_context_returns_none_without_touching_the_registry(
    cx: &mut gpui_kit::TestAppContext,
) {
    let path = temp_tunnels_path();
    cx.update(crate::runtime::init);

    let result = cx.update(|cx| acquire_for_context(cx, &path, None, "no-such-context"));
    assert!(matches!(result, Ok(None)));
    // Section 6.3: an unbound context must never initialize `TunnelForwards` -
    // `ForwardRegistry` stays untouched, not just empty.
    assert!(!cx.update(|cx| cx.has_global::<TunnelForwards>()));

    let _ = std::fs::remove_file(&path);
}

/// Section 1.2: a context bound to a tunnel acquires that tunnel's forward when
/// looked up by its own name, not the kubeconfig's `current-context` - the case
/// `connection.rs`'s `connect(cx, Some(context_name))` now relies on via
/// `resolve_bound_context`. Section 2.3: the forward's target now comes from a
/// fixture kubeconfig rather than a `remote_host`/`remote_port` on the tunnel
/// itself. Only acquisition is asserted here; the forward actually reaching `Up`
/// against a real bastion is `tunnel-bastion-verification` (deferred, blocked on a
/// real bastion to test against).
#[gpui_kit::test]
async fn bound_context_acquires_its_tunnel_by_name(cx: &mut gpui_kit::TestAppContext) {
    let tunnels_path = temp_tunnels_path();
    let kubeconfig_path = kubeconfig_fixture(&[("staging", "https://10.0.0.1:6443")]);

    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("staging-bastion", sample_tunnel("staging"), None)
        .unwrap();
    store.bind("staging", "staging-bastion").unwrap();

    cx.update(crate::runtime::init);
    let result =
        cx.update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "staging"));
    assert!(matches!(result, Ok(Some(_))));
    // A context sharing the kubeconfig's current-context, but not named "staging",
    // must not match - the binding is looked up by the exact name passed in.
    let miss = cx
        .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "not-staging"));
    assert!(matches!(miss, Ok(None)));

    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}

/// Tasks.md 2.3: a bound context whose kubeconfig server URL has no host fails the
/// acquire with that error, and never touches `TunnelForwards` - no `ssh` starts.
#[gpui_kit::test]
async fn hostless_server_fails_the_acquire_and_starts_no_ssh(cx: &mut gpui_kit::TestAppContext) {
    let tunnels_path = temp_tunnels_path();
    let kubeconfig_path = kubeconfig_fixture(&[("broken", "https://")]);

    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("some-bastion", sample_tunnel("some"), None)
        .unwrap();
    store.bind("broken", "some-bastion").unwrap();

    cx.update(crate::runtime::init);
    let result =
        cx.update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "broken"));
    assert!(matches!(
        result,
        Err(TunnelAcquireError::Target(ServerForContextError::NoHost(_)))
    ));
    assert!(
        !cx.update(|cx| cx.has_global::<TunnelForwards>()),
        "a target-resolution failure must start no ssh"
    );

    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}

/// Tasks.md 2.3: two contexts sharing one tunnel but pointing at different API
/// servers each get their own forward - `ForwardKey` includes the target, not just
/// the tunnel id, so they don't collide the way two contexts on the *same* server
/// through the same tunnel would.
#[gpui_kit::test]
async fn two_contexts_on_different_servers_through_one_tunnel_get_separate_forwards(
    cx: &mut gpui_kit::TestAppContext,
) {
    let tunnels_path = temp_tunnels_path();
    let kubeconfig_path = kubeconfig_fixture(&[
        ("cluster-a", "https://10.0.0.1:6443"),
        ("cluster-b", "https://10.0.0.2:6443"),
    ]);

    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("shared-bastion", sample_tunnel("shared"), None)
        .unwrap();
    store.bind("cluster-a", "shared-bastion").unwrap();
    store.bind("cluster-b", "shared-bastion").unwrap();

    cx.update(crate::runtime::init);
    let a = cx
        .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "cluster-a"))
        .unwrap()
        .expect("cluster-a is bound");
    let b = cx
        .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "cluster-b"))
        .unwrap()
        .expect("cluster-b is bound");

    assert_ne!(
        a.forward().local_addr(),
        b.forward().local_addr(),
        "different API servers through one tunnel must produce two forwards"
    );

    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}

/// Tasks.md 3.3: rebinding a context mid-connection never alters the connection
/// already using its old tunnel - `connect` (and so `acquire_for_context`) only
/// ever runs at connection start, so the already-acquired `RegistryHandle` simply
/// keeps pointing at its own forward regardless of what `tunnels.toml` says
/// afterward. The *next* acquire, though, picks up the new binding.
#[gpui_kit::test]
async fn rebinding_a_context_never_alters_its_live_connection(cx: &mut gpui_kit::TestAppContext) {
    let tunnels_path = temp_tunnels_path();
    let kubeconfig_path = kubeconfig_fixture(&[("staging", "https://10.0.0.1:6443")]);

    let store = TunnelStore::new(tunnels_path.clone());
    store.create("bastion-a", sample_tunnel("a"), None).unwrap();
    store.create("bastion-b", sample_tunnel("b"), None).unwrap();
    store.bind("staging", "bastion-a").unwrap();

    cx.update(crate::runtime::init);
    let handle_a = cx
        .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "staging"))
        .unwrap()
        .expect("staging is bound to bastion-a");
    let addr_a = handle_a.forward().local_addr();

    // Rebind mid-connection: nothing about the live handle changes.
    store.bind("staging", "bastion-b").unwrap();
    assert_eq!(
        handle_a.forward().local_addr(),
        addr_a,
        "the live connection must keep using bastion-a's forward after a rebind"
    );

    // Reconnecting - a fresh acquire - picks up the new binding and gets its own
    // forward, distinct from the still-live handle_a.
    let handle_b = cx
        .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "staging"))
        .unwrap()
        .expect("staging is now bound to bastion-b");
    assert_ne!(
        handle_b.forward().local_addr(),
        addr_a,
        "the next connection must acquire bastion-b's own forward"
    );

    drop(handle_a);
    drop(handle_b);
    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}

/// Section 4.1's "running state follows a fake forward's acquire and release":
/// a hand-driven `watch::Sender<BTreeSet<ForwardKey>>` stands in for
/// `ForwardRegistry::live_keys` (which only ever changes on a real acquire/release,
/// per `forward/registry.rs`'s own tests), and `drive_live_keys` must report the
/// initial snapshot immediately, then each change, in order.
#[tokio::test]
async fn drive_live_keys_reports_the_initial_set_then_each_change() {
    let key = ForwardKey::Ssh {
        tunnel_id: "qa-bastion".to_string(),
        host: "10.0.0.1".to_string(),
        port: 6443,
    };
    let (state_tx, state_rx) = watch::channel(BTreeSet::new());
    let (tx, mut rx) = mpsc::channel(4);
    let handle = tokio::spawn(drive_live_keys(state_rx, tx));

    assert_eq!(rx.recv().await, Some(BTreeSet::new()));

    // "acquire"
    state_tx
        .send(BTreeSet::from([key.clone()]))
        .expect("receiver still live");
    assert_eq!(rx.recv().await, Some(BTreeSet::from([key.clone()])));

    // "release"
    state_tx.send(BTreeSet::new()).expect("receiver still live");
    assert_eq!(rx.recv().await, Some(BTreeSet::new()));

    drop(state_tx);
    handle.await.unwrap();
    assert_eq!(rx.recv().await, None);
}

fn command_tunnel(name: &str) -> TunnelConfig {
    TunnelConfig {
        name: name.to_string(),
        kind: crate::config::tunnels::TunnelKind::Command,
        command: crate::config::tunnels::CommandTunnelConfig {
            command_line: "true {port}".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// `command-tunnels` 3.1: contexts on different API servers bound to one command
/// tunnel share one forward - one running command - keyed by the tunnel alone, and
/// take no target from their kubeconfigs.
#[gpui_kit::test]
async fn two_contexts_on_different_servers_share_one_command_tunnel(
    cx: &mut gpui_kit::TestAppContext,
) {
    let tunnels_path = temp_tunnels_path();
    let kubeconfig_path = kubeconfig_fixture(&[
        ("cluster-a", "https://10.0.0.1:6443"),
        ("cluster-b", "https://"),
    ]);
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("qa-iap", command_tunnel("qa-iap"), None)
        .unwrap();
    store.bind("cluster-a", "qa-iap").unwrap();
    store.bind("cluster-b", "qa-iap").unwrap();

    cx.update(crate::runtime::init);
    let acquire = |cx: &mut gpui_kit::TestAppContext, context: &str| {
        cx.update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), context))
            .expect("a command tunnel needs no target, so even a hostless server binds")
            .expect("bound")
    };
    let a = acquire(cx, "cluster-a");
    let b = acquire(cx, "cluster-b");

    assert_eq!(
        a.forward().local_addr(),
        b.forward().local_addr(),
        "one shared forward"
    );
    assert_eq!(
        a.forward().route(),
        TunnelRoute::Proxy,
        "proxy is the default mode"
    );
    let live = cx.update(live_forward_keys);
    assert_eq!(
        *live.borrow(),
        BTreeSet::from([ForwardKey::Command {
            tunnel_id: "qa-iap".to_string()
        }])
    );

    drop((a, b));
    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}
