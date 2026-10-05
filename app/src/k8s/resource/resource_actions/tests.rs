//! The delete helper's requests, against a server that records them: a normal
//! delete and a force kill differ only in the grace period they ask for.

use super::{ActionFailure, delete};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::test_recorder::Recorder;
use serde_json::json;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

fn deleted() -> serde_json::Value {
    json!({ "kind": "Status", "apiVersion": "v1", "status": "Success", "code": 200 })
}

#[test]
fn a_delete_asks_for_the_default_grace_period() {
    let runtime = runtime();
    let (server, client) = Recorder::start(runtime.handle(), "200 OK", deleted());

    runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            false,
        ))
        .expect("deleted");

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert!(
        requests[0]
            .target
            .starts_with("/api/v1/namespaces/shop/pods/web-1"),
        "{}",
        requests[0].target
    );
    assert_eq!(
        requests[0].json().get("gracePeriodSeconds"),
        None,
        "the pod's own grace period applies"
    );
}

#[test]
fn a_force_kill_asks_for_no_grace_period() {
    let runtime = runtime();
    let (server, client) = Recorder::start(runtime.handle(), "200 OK", deleted());

    runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            true,
        ))
        .expect("deleted");

    let requests = server.requests();
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].json()["gracePeriodSeconds"], json!(0));
}

#[test]
fn a_rejected_delete_reports_why() {
    let runtime = runtime();
    let forbidden = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "Forbidden", "code": 403,
        "message": "pods \"web-1\" is forbidden: User \"dev\" cannot delete resource \"pods\"" });
    let (_server, client) = Recorder::start(runtime.handle(), "403 Forbidden", forbidden);

    let failure: ActionFailure = runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            false,
        ))
        .expect_err("refused");

    assert!(failure.message.starts_with("Forbidden: "), "{failure:?}");
}

#[test]
fn deleting_what_is_already_gone_succeeds() {
    let runtime = runtime();
    let gone = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "NotFound", "code": 404, "message": "pods \"web-1\" not found" });
    let (_server, client) = Recorder::start(runtime.handle(), "404 Not Found", gone);

    runtime
        .block_on(delete(
            client,
            &DiscoveredKind::pods(),
            "web-1",
            Some("shop"),
            false,
        ))
        .expect("already gone is done");
}

mod editing {
    //! Section 2's request and checks: what an edit starts from, what makes
    //! edited text a manifest for the object, and the apply it is sent as.

    use super::runtime;
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::resource::resource_actions::apply::FIELD_MANAGER;
    use crate::k8s::resource::resource_actions::{apply, edit_text, parse_manifest};
    use crate::k8s::test_recorder::Recorder;
    use kube::api::DynamicObject;
    use kube::core::GroupVersionKind;
    use serde_json::json;

    fn deployments() -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
            plural: "deployments".into(),
            namespaced: true,
            verbs: Default::default(),
        }
    }

    fn deployment(replicas: i64) -> serde_json::Value {
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {
                "name": "web", "namespace": "staging", "resourceVersion": "41",
                "managedFields": [{ "manager": "kubectl", "operation": "Update" }],
            },
            "spec": { "replicas": replicas },
            "status": { "readyReplicas": replicas },
        })
    }

    #[test]
    fn an_edit_starts_without_managed_fields_or_status_but_keeps_the_version() {
        let object: DynamicObject = serde_json::from_value(deployment(2)).unwrap();
        let text = edit_text(&object);
        assert!(!text.contains("managedFields"), "{text}");
        assert!(!text.contains("status"), "{text}");
        assert!(
            text.contains("resourceVersion: '41'") || text.contains("resourceVersion: \"41\""),
            "{text}"
        );
        assert!(text.contains("replicas: 2"), "{text}");
    }

    #[test]
    fn a_well_formed_edit_is_sent_as_an_unforced_server_side_apply() {
        let runtime = runtime();
        let (server, client) = Recorder::start(runtime.handle(), "200 OK", deployment(5));
        let text =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  replicas: 5\n";
        let object = parse_manifest(text, &deployments(), "web", Some("staging")).unwrap();

        runtime
            .block_on(apply(client, &deployments(), Some("staging"), object))
            .expect("applied");

        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.method, "PATCH");
        assert!(
            request
                .target
                .starts_with("/apis/apps/v1/namespaces/staging/deployments/web?"),
            "{}",
            request.target
        );
        assert!(
            request
                .target
                .contains(&format!("fieldManager={FIELD_MANAGER}")),
            "{}",
            request.target
        );
        assert!(
            !request.target.contains("force=true"),
            "not forced: {}",
            request.target
        );
        assert_eq!(
            request.content_type.as_deref(),
            Some("application/apply-patch+yaml")
        );
        assert_eq!(request.json()["spec"]["replicas"], json!(5));
        assert_eq!(
            request.json()["metadata"]["namespace"],
            json!("staging"),
            "the namespace the edit was in"
        );
    }

    #[test]
    fn only_a_manifest_for_the_edited_object_can_be_saved() {
        let kind = deployments();
        let not_yaml = parse_manifest("spec: [unclosed", &kind, "web", Some("staging"));
        assert!(not_yaml.unwrap_err().starts_with("Not a valid manifest"));

        let other_kind = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: web\n";
        assert!(
            parse_manifest(other_kind, &kind, "web", Some("staging"))
                .unwrap_err()
                .contains("ConfigMap")
        );

        let renamed = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\n";
        assert!(
            parse_manifest(renamed, &kind, "web", Some("staging"))
                .unwrap_err()
                .contains("names api")
        );

        let moved =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  namespace: prod\n";
        assert!(
            parse_manifest(moved, &kind, "web", Some("staging"))
                .unwrap_err()
                .contains("in prod")
        );
    }
}
