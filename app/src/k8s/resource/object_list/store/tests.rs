use crate::k8s::resource::object_list::ObjectsTable;
use kube::api::{DynamicObject, ObjectMeta};
use kube_runtime::watcher;

fn object(uid: &str, name: &str, namespace: Option<&str>) -> DynamicObject {
    DynamicObject {
        types: None,
        metadata: ObjectMeta {
            uid: Some(uid.into()),
            name: Some(name.into()),
            namespace: namespace.map(Into::into),
            ..Default::default()
        },
        data: serde_json::Value::Null,
    }
}

fn names(table: &ObjectsTable) -> Vec<&str> {
    let mut names: Vec<&str> = table.rows().iter().map(|row| row.name.as_str()).collect();
    names.sort_unstable();
    names
}

#[test]
fn the_initial_list_populates_rows_with_their_base_columns() {
    let mut table = ObjectsTable::default();
    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(object(
        "u1",
        "web",
        Some("staging"),
    )));
    table.apply(watcher::Event::InitApply(object("u2", "node-a", None)));
    table.apply(watcher::Event::InitDone);

    assert_eq!(names(&table), ["node-a", "web"]);
    let web = table.rows().iter().find(|row| row.name == "web").unwrap();
    assert_eq!(web.namespace.as_deref(), Some("staging"));
    let node = table
        .rows()
        .iter()
        .find(|row| row.name == "node-a")
        .unwrap();
    assert_eq!(
        node.namespace, None,
        "a cluster-scoped object has no namespace"
    );
}

#[test]
fn apply_upserts_by_uid_and_delete_removes() {
    let mut table = ObjectsTable::default();
    table.apply(watcher::Event::Apply(object("u1", "web", Some("staging"))));
    table.apply(watcher::Event::Apply(object("u2", "api", Some("staging"))));
    // The same uid again replaces, it doesn't add.
    table.apply(watcher::Event::Apply(object(
        "u1",
        "web-renamed",
        Some("staging"),
    )));
    assert_eq!(names(&table), ["api", "web-renamed"]);

    table.apply(watcher::Event::Delete(object("u2", "api", Some("staging"))));
    assert_eq!(names(&table), ["web-renamed"]);
}

#[test]
fn a_restart_relist_sweeps_objects_deleted_while_the_watch_was_down() {
    let mut table = ObjectsTable::default();
    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(object(
        "u1",
        "web",
        Some("staging"),
    )));
    table.apply(watcher::Event::InitApply(object(
        "u2",
        "api",
        Some("staging"),
    )));
    table.apply(watcher::Event::InitDone);

    // The watch reconnects; `api` was deleted meanwhile, so the relist omits it.
    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(object(
        "u1",
        "web",
        Some("staging"),
    )));
    table.apply(watcher::Event::InitDone);

    assert_eq!(names(&table), ["web"]);
}

#[test]
fn a_refusal_is_kept_until_the_list_is_served_again() {
    let mut table = ObjectsTable::default();
    table.set_refused("secrets is forbidden".into());
    assert_eq!(table.refused(), Some("secrets is forbidden"));
    assert!(table.rows().is_empty());

    table.apply(watcher::Event::Init);
    assert_eq!(table.refused(), None);
}
