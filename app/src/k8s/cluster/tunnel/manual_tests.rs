//! `manual-confirmation-tunnels` 2.2: a manual tunnel through the real tunnel
//! registry and the app's `ManualConfirmations` - one shared prompt, Proceed,
//! Cancel, an already-confirmed tunnel, and prompting again once released.

use super::*;
use crate::config::tunnels::{ManualTunnelConfig, TunnelConfig};
use crate::tunnel::manual::{Decision, ManualConfirmations};
use gpui_kit::TestAppContext;

type Handle = RegistryHandle<ForwardKey, TunnelForward>;

/// A tunnels file with one manual tunnel, `corp-vpn`, bound to `contexts`, and a
/// kubeconfig whose contexts all point at an address nothing answers on. The
/// shortcut is off, so every confirmation prompts.
fn fixture(contexts: &[&str]) -> (std::path::PathBuf, std::path::PathBuf) {
    let tunnels = crate::util::test_paths::temp_path("manual-tunnel");
    let store = TunnelStore::new(tunnels.clone());
    store
        .create(
            "corp-vpn-id",
            TunnelConfig {
                name: "corp-vpn".into(),
                kind: TunnelKind::Manual,
                manual: ManualTunnelConfig {
                    message: Some("Connect the corporate VPN".into()),
                    skip_when_reachable: false,
                },
                ..TunnelConfig::default()
            },
            None,
        )
        .unwrap();
    for context in contexts {
        store.bind(context, "corp-vpn-id").unwrap();
    }
    let kubeconfig = crate::util::test_paths::temp_path("manual-kubeconfig").with_extension("yaml");
    let entries = |f: &dyn Fn(&str) -> String| contexts.iter().map(|c| f(c)).collect::<String>();
    let yaml = format!(
        "apiVersion: v1\nkind: Config\nclusters:\n{}contexts:\n{}users:\n{}",
        entries(&|c| format!("  - name: {c}\n    cluster:\n      server: https://127.0.0.1:1\n")),
        entries(&|c| format!("  - name: {c}\n    context:\n      cluster: {c}\n      user: {c}\n")),
        entries(&|c| format!("  - name: {c}\n    user: {{}}\n")),
    );
    std::fs::write(&kubeconfig, yaml).unwrap();
    (tunnels, kubeconfig)
}

fn setup(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
}

fn acquire(
    cx: &mut TestAppContext,
    paths: &(std::path::PathBuf, std::path::PathBuf),
    context: &str,
) -> Handle {
    cx.update(|cx| acquire_for_context(cx, &paths.0, Some(&paths.1), context))
        .expect("the acquire succeeds")
        .expect("the context is bound")
}

/// Runs the app until `condition` holds, failing after a couple of seconds.
fn until(
    cx: &mut TestAppContext,
    what: &str,
    mut condition: impl FnMut(&mut TestAppContext) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out waiting for {what}");
}

/// The pending confirmations, as (tunnel name, waiting contexts).
fn pending(cx: &mut TestAppContext) -> Vec<(String, Vec<String>)> {
    cx.update(|cx| {
        ManualConfirmations::entity(cx)
            .map(|entity| {
                entity
                    .read(cx)
                    .pending()
                    .iter()
                    .map(|entry| (entry.name.clone(), entry.contexts.clone()))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn state(handle: &Handle) -> ForwardState {
    *handle.forward().state().borrow()
}

fn resolve(cx: &mut TestAppContext, decision: Decision) -> bool {
    cx.update(|cx| ManualConfirmations::resolve(cx, "corp-vpn-id", decision))
}

#[gpui_kit::test]
async fn two_contexts_share_one_prompt_and_one_proceed_releases_both(cx: &mut TestAppContext) {
    setup(cx);
    let paths = fixture(&["dev", "qa"]);
    let dev = acquire(cx, &paths, "dev");
    until(cx, "the prompt", |cx| !pending(cx).is_empty());
    let qa = acquire(cx, &paths, "qa");
    cx.run_until_parked();

    assert_eq!(
        pending(cx),
        vec![(
            "corp-vpn".to_string(),
            vec!["dev".to_string(), "qa".to_string()]
        )]
    );
    let message = cx.update(|cx| {
        ManualConfirmations::entity(cx).unwrap().read(cx).pending()[0]
            .message
            .clone()
    });
    assert_eq!(message.as_deref(), Some("Connect the corporate VPN"));
    assert_ne!(state(&dev), ForwardState::Up);

    assert!(resolve(cx, Decision::Proceed));
    until(cx, "both connections released", |_| {
        state(&dev) == ForwardState::Up && state(&qa) == ForwardState::Up
    });
    assert!(pending(cx).is_empty());
    drop((dev, qa));
}

#[gpui_kit::test]
async fn cancel_fails_every_waiter_with_the_tunnels_reason(cx: &mut TestAppContext) {
    setup(cx);
    let paths = fixture(&["dev", "qa"]);
    let dev = acquire(cx, &paths, "dev");
    let qa = acquire(cx, &paths, "qa");
    until(cx, "the prompt", |cx| !pending(cx).is_empty());

    assert!(resolve(cx, Decision::Cancel));
    for handle in [&dev, &qa] {
        let state = handle.forward().state();
        let failure = handle.forward().failure();
        until(cx, "the forward to close", |_| {
            state.has_changed().is_err() && failure.get().is_some()
        });
        assert_eq!(failure.get().as_deref(), Some("corp-vpn was cancelled"));
    }
    assert!(pending(cx).is_empty());
}

#[gpui_kit::test]
async fn a_confirmed_tunnel_lets_another_context_through_without_a_prompt(cx: &mut TestAppContext) {
    setup(cx);
    let paths = fixture(&["dev", "qa"]);
    let dev = acquire(cx, &paths, "dev");
    until(cx, "the prompt", |cx| !pending(cx).is_empty());
    resolve(cx, Decision::Proceed);
    until(cx, "dev released", |_| state(&dev) == ForwardState::Up);

    let qa = acquire(cx, &paths, "qa");
    assert_eq!(state(&qa), ForwardState::Up, "already confirmed");
    cx.run_until_parked();
    assert!(pending(cx).is_empty(), "no new prompt");
    drop((dev, qa));
}

#[gpui_kit::test]
async fn releasing_every_connection_makes_the_next_one_prompt_again(cx: &mut TestAppContext) {
    setup(cx);
    let paths = fixture(&["dev"]);
    let dev = acquire(cx, &paths, "dev");
    until(cx, "the prompt", |cx| !pending(cx).is_empty());
    resolve(cx, Decision::Proceed);
    until(cx, "dev released", |_| state(&dev) == ForwardState::Up);

    // The last connection goes - disconnected, or failed after Proceed, which
    // lets its forward go too (`connection::releases_forward_on`).
    drop(dev);
    let again = acquire(cx, &paths, "dev");
    assert_ne!(state(&again), ForwardState::Up);
    until(cx, "a fresh prompt", |cx| pending(cx).len() == 1);
    assert_eq!(pending(cx)[0].1, vec!["dev".to_string()]);
}

#[gpui_kit::test]
async fn a_prompt_goes_away_with_its_last_waiter(cx: &mut TestAppContext) {
    setup(cx);
    let paths = fixture(&["dev"]);
    let dev = acquire(cx, &paths, "dev");
    until(cx, "the prompt", |cx| !pending(cx).is_empty());
    drop(dev);
    until(cx, "the prompt withdrawn", |cx| pending(cx).is_empty());
    assert!(!resolve(cx, Decision::Proceed), "nothing left to resolve");
}

#[gpui_kit::test]
async fn a_manual_tunnel_rewrites_nothing(cx: &mut TestAppContext) {
    setup(cx);
    let paths = fixture(&["dev"]);
    let dev = acquire(cx, &paths, "dev");
    assert_eq!(dev.forward().route(), TunnelRoute::Direct);
}

/// `manual-confirmation-tunnels` 2.3: with the shortcut on, a context whose API
/// server already accepts a TCP connection goes through without a prompt.
#[gpui_kit::test]
async fn a_reachable_api_server_skips_the_prompt(cx: &mut TestAppContext) {
    setup(cx);
    // Listening, never accepting: the connect still succeeds off the backlog.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tunnels, kubeconfig) = fixture(&["dev"]);
    let store = TunnelStore::new(tunnels.clone());
    let mut tunnel = store.get("corp-vpn-id").unwrap();
    tunnel.manual.skip_when_reachable = true;
    store.update("corp-vpn-id", tunnel, None).unwrap();
    let yaml = std::fs::read_to_string(&kubeconfig)
        .unwrap()
        .replace("127.0.0.1:1", &format!("127.0.0.1:{port}"));
    std::fs::write(&kubeconfig, yaml).unwrap();

    let dev = acquire(cx, &(tunnels, kubeconfig), "dev");
    until(cx, "the tunnel confirmed by reachability", |_| {
        state(&dev) == ForwardState::Up
    });
    assert!(pending(cx).is_empty(), "no prompt");
    drop((dev, listener));
}

/// With the shortcut on but the API server not answering, the user is asked.
#[gpui_kit::test]
async fn an_unreachable_api_server_still_prompts(cx: &mut TestAppContext) {
    setup(cx);
    let (tunnels, kubeconfig) = fixture(&["dev"]);
    let store = TunnelStore::new(tunnels.clone());
    let mut tunnel = store.get("corp-vpn-id").unwrap();
    tunnel.manual.skip_when_reachable = true;
    store.update("corp-vpn-id", tunnel, None).unwrap();

    let dev = acquire(cx, &(tunnels, kubeconfig), "dev");
    until(cx, "the prompt", |cx| pending(cx).len() == 1);
    assert_ne!(state(&dev), ForwardState::Up);
}
