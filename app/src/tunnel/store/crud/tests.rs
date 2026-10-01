use crate::config::{self, tunnels::TunnelsConfig};
use crate::tunnel::store::test_support::{next_id, sample_tunnel, temp_config_path};
use crate::tunnel::store::{TunnelFieldError, TunnelStore, TunnelStoreError};

#[test]
fn create_writes_config_and_secret() {
    let id = format!("prod-bastion-{}", next_id());
    let store = TunnelStore::new(temp_config_path());
    store
        .create(&id, sample_tunnel("Prod"), Some("s3cr3t"))
        .unwrap();

    let listed = store.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].0, id);
    assert_eq!(store.secrets.read(&id).unwrap(), Some("s3cr3t".to_string()));

    store.secrets.delete(&id).unwrap();
}

#[test]
fn create_with_existing_id_fails() {
    let store = TunnelStore::new(temp_config_path());
    store.create("dup", sample_tunnel("First"), None).unwrap();
    let err = store
        .create("dup", sample_tunnel("Second"), None)
        .unwrap_err();
    assert!(matches!(err, TunnelStoreError::AlreadyExists));
}

#[test]
fn update_renames_without_changing_id_or_stored_secret_when_none_passed() {
    let id = format!("stable-id-{}", next_id());
    let store = TunnelStore::new(temp_config_path());
    store
        .create(&id, sample_tunnel("Old Name"), Some("s3cr3t"))
        .unwrap();

    store.update(&id, sample_tunnel("New Name"), None).unwrap();

    let listed = store.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].0, id);
    assert_eq!(listed[0].1.name, "New Name");
    assert_eq!(
        store.secrets.read(&id).unwrap(),
        Some("s3cr3t".to_string()),
        "update without a secret must not clear the previously stored one"
    );

    store.secrets.delete(&id).unwrap();
}

#[test]
fn update_of_missing_id_fails() {
    let store = TunnelStore::new(temp_config_path());
    let err = store
        .update("missing", sample_tunnel("Name"), None)
        .unwrap_err();
    assert!(matches!(err, TunnelStoreError::NotFound));
}

/// The unbind cascade required by tasks.md 5.3: deleting a tunnel must unbind every
/// context that pointed at it and report which ones, while leaving bindings to
/// other tunnels untouched.
#[test]
fn delete_unbinds_every_context_pointing_at_the_tunnel() {
    let path = temp_config_path();
    let store = TunnelStore::new(path.clone());
    store
        .create("prod-bastion", sample_tunnel("Prod"), Some("s3cr3t"))
        .unwrap();
    store
        .create("staging-bastion", sample_tunnel("Staging"), None)
        .unwrap();

    let mut config: TunnelsConfig = config::load(&path);
    config
        .context_bindings
        .insert("prod-east".to_string(), "prod-bastion".to_string());
    config
        .context_bindings
        .insert("prod-west".to_string(), "prod-bastion".to_string());
    config
        .context_bindings
        .insert("staging".to_string(), "staging-bastion".to_string());
    config::save(&path, &config).unwrap();

    let mut unbound = store.delete("prod-bastion").unwrap();
    unbound.sort();
    assert_eq!(
        unbound,
        vec!["prod-east".to_string(), "prod-west".to_string()]
    );

    let after = store.list();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].0, "staging-bastion");

    let after_config: TunnelsConfig = config::load(&path);
    assert_eq!(
        after_config.context_bindings.get("staging"),
        Some(&"staging-bastion".to_string()),
        "an unrelated binding must survive the delete"
    );
    assert!(!after_config.context_bindings.contains_key("prod-east"));
    assert!(!after_config.context_bindings.contains_key("prod-west"));

    assert_eq!(store.secrets.read("prod-bastion").unwrap(), None);
}

#[test]
fn delete_of_missing_id_fails() {
    let store = TunnelStore::new(temp_config_path());
    let err = store.delete("missing").unwrap_err();
    assert!(matches!(err, TunnelStoreError::NotFound));
}

/// Tasks.md 1.2: `create`/`update` reject an empty `bastion_host`, naming the field.
#[test]
fn create_rejects_an_empty_host() {
    let store = TunnelStore::new(temp_config_path());
    let mut tunnel = sample_tunnel("Bad");
    tunnel.bastion_host = String::new();

    let err = store.create("bad", tunnel, None).unwrap_err();

    assert!(matches!(
        err,
        TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::EmptyHost]
    ));
    assert!(store.list().is_empty(), "an invalid tunnel must not save");
}

/// Tasks.md 1.2: an empty `bastion_user` is rejected the same way.
#[test]
fn create_rejects_an_empty_user() {
    let store = TunnelStore::new(temp_config_path());
    let mut tunnel = sample_tunnel("Bad");
    tunnel.bastion_user = String::new();

    let err = store.create("bad", tunnel, None).unwrap_err();

    assert!(matches!(
        err,
        TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::EmptyUser]
    ));
}

/// Tasks.md 1.2: `bastion_port` must be in 1-65535; `u16` already caps the top, so
/// only `0` is out of range.
#[test]
fn create_rejects_an_out_of_range_port() {
    let store = TunnelStore::new(temp_config_path());
    let mut tunnel = sample_tunnel("Bad");
    tunnel.bastion_port = 0;

    let err = store.create("bad", tunnel, None).unwrap_err();

    assert!(matches!(
        err,
        TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::InvalidPort]
    ));
}

/// `update` validates too, and rejects before touching the file.
#[test]
fn update_rejects_an_invalid_tunnel() {
    let store = TunnelStore::new(temp_config_path());
    store.create("a", sample_tunnel("A"), None).unwrap();
    let mut invalid = sample_tunnel("A renamed");
    invalid.bastion_user = String::new();

    let err = store.update("a", invalid, None).unwrap_err();

    assert!(matches!(
        err,
        TunnelStoreError::Invalid(errors) if errors == vec![TunnelFieldError::EmptyUser]
    ));
    assert_eq!(store.get("a").unwrap().name, "A");
}
