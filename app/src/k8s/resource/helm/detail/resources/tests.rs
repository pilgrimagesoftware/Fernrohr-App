//! helm-releases section 6: the Resources tab. What a manifest declares, and
//! over a fake cluster, by keystrokes: the tab by its key, each object marked
//! present or missing, a cluster-scoped one without a namespace, and Enter
//! following the selected object as a `resource-links` reference.

use super::{Presence, parse_manifest, scoped};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::helm::ReleaseKey;
use crate::k8s::resource::helm::detail::HelmReleasePanel;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::link::FollowReference;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use base64::Engine as _;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Focusable as _, TestAppContext, VisualTestContext};
use kube::api::GroupVersionKind;
use serde_json::json;
use std::cell::RefCell;
use std::io::Write as _;
use std::rc::Rc;

const CONTEXT: &str = "kind-dev";

/// A Service and Deployment that exist, a ConfigMap that doesn't, and a
/// ClusterRole - cluster-scoped, so it takes no namespace.
const MANIFEST: &str = "---\n# Source: web/templates/service.yaml\napiVersion: v1\nkind: Service\n\
metadata:\n  name: web\n---\n# Source: web/templates/deployment.yaml\napiVersion: apps/v1\n\
kind: Deployment\nmetadata:\n  name: web\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  \
name: web-config\n---\napiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\n\
metadata:\n  name: web-reader\n---\n: not yaml: [\n";

fn kind(group: &str, kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: Default::default(),
    }
}

fn kinds() -> Vec<DiscoveredKind> {
    vec![
        kind("", "Service", "services", true),
        kind("apps", "Deployment", "deployments", true),
        kind("", "ConfigMap", "configmaps", true),
        kind(
            "rbac.authorization.k8s.io",
            "ClusterRole",
            "clusterroles",
            false,
        ),
    ]
}

/// Each object with its group from `apiVersion` and the release's namespace
/// unless it names its own; `List` items too; documents without a kind or a
/// name skipped. A document that doesn't parse costs only itself - the
/// objects after it are still listed - and is counted, so the tab can say the
/// list may be incomplete.
#[test]
fn a_manifest_lists_its_objects() {
    let manifest = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\n---\n\
                    : not yaml: [\n---\n\
                    apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  namespace: edge\n\
                    ---\n# Source: empty.yaml\n---\napiVersion: v1\nkind: ConfigMap\nmetadata: {}\n\
                    ---\napiVersion: v1\nkind: List\nitems:\n- apiVersion: v1\n  kind: Secret\n  \
                    metadata:\n    name: web-tls\n";
    let parsed = parse_manifest(manifest, "shop");
    assert_eq!(
        parsed.targets,
        [
            ObjectRef::namespaced("", "Service", "shop", "web"),
            ObjectRef::namespaced("apps", "Deployment", "edge", "web"),
            ObjectRef::namespaced("", "Secret", "shop", "web-tls"),
        ]
    );
    assert_eq!(parsed.unreadable, 1, "the broken document");
}

/// A kind discovery reports as cluster-scoped loses the release's namespace.
#[test]
fn a_cluster_scoped_kind_has_no_namespace() {
    let role = ObjectRef::namespaced(
        "rbac.authorization.k8s.io",
        "ClusterRole",
        "shop",
        "web-reader",
    );
    assert_eq!(
        scoped(role, &kinds()),
        ObjectRef::cluster_scoped("rbac.authorization.k8s.io", "ClusterRole", "web-reader")
    );
    let service = ObjectRef::namespaced("", "Service", "shop", "web");
    assert_eq!(
        scoped(service.clone(), &kinds()),
        service,
        "namespaced stays"
    );
}

fn deployed_revision() -> serde_json::Value {
    let release = json!({
        "name": "web", "namespace": "shop", "version": 1,
        "info": { "status": "deployed" },
        "chart": { "metadata": { "name": "web", "version": "1.0.0" } },
        "manifest": MANIFEST,
    });
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(release.to_string().as_bytes()).unwrap();
    let helm = base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap());
    json!({
        "apiVersion": "v1", "kind": "Secret", "type": "helm.sh/release.v1",
        "metadata": {
            "name": "sh.helm.release.v1.web.v1", "namespace": "shop",
            "labels": { "owner": "helm", "name": "web", "version": "1", "status": "deployed" },
        },
        "data": { "release": base64::engine::general_purpose::STANDARD.encode(helm) },
    })
}

#[gpui_kit::test]
async fn the_resources_tab_lists_marks_and_follows(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let followed: Rc<RefCell<Vec<ObjectRef>>> = Rc::default();
    let sink = followed.clone();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::helm::detail::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        cx.on_action(move |action: &FollowReference, _cx| {
            sink.borrow_mut().push(action.target.clone())
        });
        DiscoveryRegistry::insert_test(cx, CONTEXT, kinds());
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "secrets", deployed_revision());
    cluster.apply(
        "/api/v1",
        "services",
        json!({ "apiVersion": "v1", "kind": "Service", "metadata": { "name": "web", "namespace": "shop" } }),
    );
    cluster.apply(
        "/apis/apps/v1",
        "deployments",
        json!({ "apiVersion": "apps/v1", "kind": "Deployment", "metadata": { "name": "web", "namespace": "shop" } }),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let key = ReleaseKey {
        namespace: "shop".into(),
        name: "web".into(),
    };
    let scope = PanelScope::new(NavTarget::HelmRelease(key.clone()), CONTEXT.into());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| HelmReleasePanel::new(key, scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let focus = panel.clone();
    vcx.update(|window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
    vcx.run_until_parked();

    vcx.simulate_keystrokes("6");
    let rows = {
        let panel = panel.clone();
        // Generous: a whole `cargo test` runs many fake clusters at once. A
        // passing wait returns as soon as the rows are in.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            vcx.run_until_parked();
            let rows = vcx.update(|_, cx| panel.read(cx).resource_rows().to_vec());
            if rows.len() == 4 || std::time::Instant::now() > deadline {
                break rows;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    let listed: Vec<(&str, Option<&str>, Presence)> = rows
        .iter()
        .map(|row| {
            (
                row.target.kind.as_str(),
                row.target.namespace.as_deref(),
                row.presence,
            )
        })
        .collect();
    assert_eq!(listed[0], ("Service", Some("shop"), Presence::Present));
    assert_eq!(listed[1], ("Deployment", Some("shop"), Presence::Present));
    assert_eq!(listed[2], ("ConfigMap", Some("shop"), Presence::Missing));
    assert_eq!(listed[3].0, "ClusterRole");
    assert_eq!(listed[3].1, None, "cluster-scoped: no namespace");
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert!(
        vcx.debug_bounds(super::view::UNREADABLE_NOTE).is_some(),
        "the broken document is noted, so the list doesn't pass for complete"
    );

    vcx.simulate_keystrokes("down enter");
    vcx.run_until_parked();
    assert_eq!(
        followed.borrow().last(),
        Some(&ObjectRef::namespaced("apps", "Deployment", "shop", "web")),
        "Enter follows the selected Deployment"
    );
    vcx.simulate_keystrokes("down enter");
    vcx.run_until_parked();
    assert_eq!(
        followed.borrow().last(),
        Some(&ObjectRef::namespaced(
            "",
            "ConfigMap",
            "shop",
            "web-config"
        )),
        "a missing object still follows - to its not-found panel"
    );
}
