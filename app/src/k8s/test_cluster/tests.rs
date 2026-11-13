//! The fake cluster's selectors and metadata-only answers (helm-releases 2.1),
//! through a real kube client.

use super::FakeCluster;
use gpui_kit::TestAppContext;
use k8s_openapi::api::core::v1::Secret;
use kube::Api;
use kube::api::ListParams;
use kube::core::PartialObjectMeta;
use serde_json::json;

fn secret(name: &str, kind: &str, owner: Option<&str>) -> serde_json::Value {
    let mut secret = json!({ "apiVersion": "v1", "kind": "Secret", "type": kind,
        "metadata": { "name": name, "namespace": "shop" }, "data": { "key": "dmFsdWU=" } });
    if let Some(owner) = owner {
        secret["metadata"]["labels"] = json!({ "owner": owner });
    }
    secret
}

fn names<T>(items: &[T], name: impl Fn(&T) -> Option<String>) -> Vec<String> {
    let mut names: Vec<String> = items.iter().filter_map(name).collect();
    names.sort();
    names
}

#[gpui_kit::test]
async fn lists_honor_label_and_type_selectors(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "secrets",
        secret("a", "helm.sh/release.v1", Some("helm")),
    );
    cluster.apply("/api/v1", "secrets", secret("b", "Opaque", Some("helm")));
    cluster.apply(
        "/api/v1",
        "secrets",
        secret("c", "helm.sh/release.v1", None),
    );
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let api: Api<Secret> = Api::namespaced(client.clone(), "shop");

    let by_label = handle
        .block_on(api.list(&ListParams::default().labels("owner==helm")))
        .unwrap();
    assert_eq!(
        names(&by_label.items, |s| s.metadata.name.clone()),
        ["a", "b"]
    );
    let by_both = handle
        .block_on(
            api.list(
                &ListParams::default()
                    .labels("owner=helm")
                    .fields("type=helm.sh/release.v1"),
            ),
        )
        .unwrap();
    assert_eq!(names(&by_both.items, |s| s.metadata.name.clone()), ["a"]);
    assert_eq!(cluster.list_requests(), (0, 2), "whole objects, twice");
}

#[gpui_kit::test]
async fn a_metadata_list_carries_no_data(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "secrets",
        secret("a", "helm.sh/release.v1", Some("helm")),
    );
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let api: Api<PartialObjectMeta<Secret>> = Api::namespaced(client, "shop");
    let listed = handle.block_on(api.list(&ListParams::default())).unwrap();
    assert_eq!(names(&listed.items, |s| s.metadata.name.clone()), ["a"]);
    assert_eq!(cluster.list_requests(), (1, 0), "asked for metadata only");
}
