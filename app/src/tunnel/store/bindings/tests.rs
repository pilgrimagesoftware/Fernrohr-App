use crate::tunnel::store::test_support::{sample_tunnel, temp_config_path};
use crate::tunnel::store::{TunnelStore, TunnelStoreError};

/// Tasks.md 6.1's "bind and persist" scenario: a binding written by one `TunnelStore`
/// is visible to a fresh one reading the same file, standing in for a relaunch.
#[test]
fn bind_persists_across_a_fresh_store_over_the_same_file() {
    let path = temp_config_path();
    let store = TunnelStore::new(path.clone());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();
    store.bind("qa-1", "qa-bastion").unwrap();

    let reopened = TunnelStore::new(path);
    assert_eq!(
        reopened.bindings(),
        vec![("qa-1".to_string(), "qa-bastion".to_string())]
    );
}

/// Tasks.md 6.1's "shared tunnel" scenario: many contexts may point at one tunnel id.
#[test]
fn many_contexts_can_share_one_tunnel() {
    let store = TunnelStore::new(temp_config_path());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();
    store.bind("qa-1", "qa-bastion").unwrap();
    store.bind("qa-2", "qa-bastion").unwrap();

    let mut bindings = store.bindings();
    bindings.sort();
    assert_eq!(
        bindings,
        vec![
            ("qa-1".to_string(), "qa-bastion".to_string()),
            ("qa-2".to_string(), "qa-bastion".to_string()),
        ]
    );
}

/// A context binds to at most one tunnel: rebinding overwrites, it never accumulates.
#[test]
fn rebinding_a_context_overwrites_its_prior_binding() {
    let store = TunnelStore::new(temp_config_path());
    store.create("a", sample_tunnel("A"), None).unwrap();
    store.create("b", sample_tunnel("B"), None).unwrap();
    store.bind("ctx", "a").unwrap();
    store.bind("ctx", "b").unwrap();

    assert_eq!(store.bindings(), vec![("ctx".to_string(), "b".to_string())]);
}

#[test]
fn bind_to_a_missing_tunnel_fails() {
    let store = TunnelStore::new(temp_config_path());
    let err = store.bind("ctx", "missing").unwrap_err();
    assert!(matches!(err, TunnelStoreError::NotFound));
    assert!(store.bindings().is_empty());
}

/// Tasks.md 6.1's "unbind" scenario.
#[test]
fn unbind_removes_the_binding() {
    let store = TunnelStore::new(temp_config_path());
    store.create("a", sample_tunnel("A"), None).unwrap();
    store.bind("ctx", "a").unwrap();

    store.unbind("ctx").unwrap();

    assert!(store.bindings().is_empty());
}

#[test]
fn unbind_of_an_unbound_context_is_a_no_op() {
    let store = TunnelStore::new(temp_config_path());
    store.unbind("never-bound").unwrap();
    assert!(store.bindings().is_empty());
}

/// Tasks.md 1.2: `usage_counts` reflects bindings shared across contexts, and
/// includes an unused tunnel at `0` rather than omitting it.
#[test]
fn usage_counts_reflects_shared_and_unused_tunnels() {
    let store = TunnelStore::new(temp_config_path());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();
    store
        .create("unused-bastion", sample_tunnel("Unused"), None)
        .unwrap();
    store.bind("qa-1", "qa-bastion").unwrap();
    store.bind("qa-2", "qa-bastion").unwrap();

    let counts = store.usage_counts();

    assert_eq!(counts.get("qa-bastion"), Some(&2));
    assert_eq!(counts.get("unused-bastion"), Some(&0));
}

/// Tasks.md 1.2: a binding whose context is absent from the given list is stale.
#[test]
fn stale_bindings_lists_contexts_absent_from_the_given_list() {
    let store = TunnelStore::new(temp_config_path());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();
    store.bind("qa-1", "qa-bastion").unwrap();
    store.bind("renamed-away", "qa-bastion").unwrap();

    let stale = store.stale_bindings(&["qa-1".to_string()]);

    assert_eq!(
        stale,
        vec![("renamed-away".to_string(), "qa-bastion".to_string())]
    );
}
