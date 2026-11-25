//! #209: what a detail panel's object controls - matched by owner uid across
//! its child watches and the context's open ones, kept live, and drawn as
//! linked "Controls" references.

use super::fixtures::{deployments, kind, object, replica_sets, stub_panel, target};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::controlled::{
    child_kinds, controlled_by, unlistable_kinds,
};
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_list::{ObjectRow, ObjectsTable};
use crate::k8s::resource::pods::PodsTable;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use k8s_openapi::api::core::v1::Pod;
use kube::api::DynamicObject;
use kube::runtime::watcher;

/// A namespaced `kind` object named `name` in `staging`, owned by `owners`.
fn owned(kind: &str, name: &str, owners: &[&str]) -> DynamicObject {
    let owners: Vec<serde_json::Value> = owners
        .iter()
        .map(|uid| {
            serde_json::json!({
                "apiVersion": "apps/v1", "kind": "Owner", "name": "owner", "uid": uid,
            })
        })
        .collect();
    object(serde_json::json!({
        "apiVersion": "apps/v1",
        "kind": kind,
        "metadata": {
            "name": name,
            "namespace": "staging",
            "uid": format!("uid-{name}"),
            "ownerReferences": owners,
        },
    }))
}

fn pod(name: &str, owner: &str) -> Pod {
    serde_json::from_value(serde_json::json!({
        "metadata": {
            "name": name,
            "namespace": "staging",
            "uid": format!("uid-{name}"),
            "ownerReferences": [{
                "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "rs", "uid": owner,
            }],
        },
    }))
    .expect("a valid pod")
}

fn rows(objects: &[DynamicObject]) -> Vec<ObjectRow> {
    objects
        .iter()
        .map(|object| ObjectRow::new(object, None))
        .collect()
}

#[test]
fn known_controllers_name_their_child_kinds() {
    assert_eq!(child_kinds("apps", "Deployment"), &[("apps", "ReplicaSet")]);
    assert_eq!(child_kinds("batch", "CronJob"), &[("batch", "Job")]);
    for (group, controller) in [
        ("apps", "ReplicaSet"),
        ("apps", "StatefulSet"),
        ("apps", "DaemonSet"),
        ("batch", "Job"),
    ] {
        assert_eq!(
            child_kinds(group, controller),
            &[("", "Pod")],
            "{controller}"
        );
    }
    assert!(child_kinds("", "ConfigMap").is_empty());
    assert!(
        child_kinds("example.com", "Deployment").is_empty(),
        "group counts"
    );
}

#[test]
fn children_match_by_owner_uid_across_kinds_and_pods() {
    let replica_sets_kind = replica_sets();
    let widgets = kind("example.com", "v1", "Widget", true);
    let sets = rows(&[
        owned("ReplicaSet", "web-b", &["deploy-1"]),
        owned("ReplicaSet", "web-a", &["deploy-1"]),
        owned("ReplicaSet", "other", &["deploy-2"]),
    ]);
    let custom = rows(&[owned("Widget", "w", &["other-owner", "deploy-1"])]);
    let pods = [pod("web-a-1", "deploy-1"), pod("stray", "deploy-9")];

    let found = controlled_by(
        "deploy-1",
        [
            (&widgets, custom.as_slice()),
            (&replica_sets_kind, sets.as_slice()),
        ],
        &pods,
    );

    assert_eq!(
        found,
        vec![
            ObjectRef::core("Pod", "staging", "web-a-1"),
            ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web-a"),
            ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web-b"),
            ObjectRef::namespaced("example.com", "Widget", "staging", "w"),
        ],
        "every kind's children, sorted by kind then name"
    );
}

#[test]
fn an_object_without_a_uid_controls_nothing() {
    let kind = replica_sets();
    let sets = rows(&[owned("ReplicaSet", "orphan", &[])]);
    assert!(controlled_by("", [(&kind, sets.as_slice())], &[]).is_empty());
}

fn synced_table(
    kind: &crate::k8s::cluster::discovery::DiscoveredKind,
    objects: Vec<DynamicObject>,
) -> ObjectsTable {
    let mut table = ObjectsTable::for_kind(kind);
    table.apply(watcher::Event::Init);
    for object in objects {
        table.apply(watcher::Event::InitApply(object));
    }
    table.apply(watcher::Event::InitDone);
    table
}

fn controls(
    panel: &Entity<crate::k8s::resource::object_detail::ObjectDetailPanel>,
    cx: &mut gpui_kit::VisualTestContext,
) -> Vec<ObjectRef> {
    cx.update(|_, cx| {
        let sections = panel.read(cx).sections(jiff::Timestamp::now());
        sections[0]
            .fields
            .iter()
            .find(|field| field.label == "Controls")
            .map(|field| match &field.value {
                FieldValue::References { targets, .. } => targets.clone(),
                other => panic!("Controls is references, not {other:?}"),
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
async fn a_deployment_lists_its_replica_sets_live_and_linked(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (window, panel) = stub_panel(
        cx,
        target(deployments(), Some("staging"), "web"),
        vec![deployments(), replica_sets()],
    );
    let mut vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    let sets = vcx.update(|_, cx| {
        cx.new(|_| {
            synced_table(
                &replica_sets(),
                vec![owned("ReplicaSet", "web-1", &["deploy-1"])],
            )
        })
    });
    vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.test_set_loaded(
                object(serde_json::json!({
                    "apiVersion": "apps/v1",
                    "kind": "Deployment",
                    "metadata": { "name": "web", "namespace": "staging", "uid": "deploy-1" },
                })),
                cx,
            );
            panel.test_watch_controlled(None, vec![(replica_sets(), sets.clone())], cx);
        })
    });
    assert_eq!(
        controls(&panel, &mut vcx),
        vec![ObjectRef::namespaced(
            "apps",
            "ReplicaSet",
            "staging",
            "web-1"
        )]
    );

    // A rollout's new ReplicaSet shows without reopening the panel.
    vcx.update(|_, cx| {
        sets.update(cx, |table, cx| {
            table.apply(watcher::Event::Apply(owned(
                "ReplicaSet",
                "web-2",
                &["deploy-1"],
            )));
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert_eq!(
        controls(&panel, &mut vcx).len(),
        2,
        "the new ReplicaSet is listed"
    );

    // Each entry is a link the "Go to…" picker offers, as Controlled By's are.
    let followable = vcx.update(|_, cx| panel.read(cx).followable(cx));
    assert!(
        followable
            .iter()
            .any(|entry| format!("{entry:?}").contains("web-2")),
        "the controlled ReplicaSet is followable: {followable:?}"
    );
}

#[gpui_kit::test]
async fn a_replica_set_lists_its_pods(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (window, panel) = stub_panel(
        cx,
        target(replica_sets(), Some("staging"), "web-1"),
        vec![replica_sets()],
    );
    let mut vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    let pods = vcx.update(|_, cx| {
        cx.new(|_| {
            let mut table = PodsTable::new();
            table.apply(watcher::Event::Init);
            table.apply(watcher::Event::InitApply(pod("web-1-a", "rs-1")));
            table.apply(watcher::Event::InitApply(pod("unrelated", "rs-9")));
            table.apply(watcher::Event::InitDone);
            table
        })
    });
    vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.test_set_loaded(
                object(serde_json::json!({
                    "apiVersion": "apps/v1",
                    "kind": "ReplicaSet",
                    "metadata": { "name": "web-1", "namespace": "staging", "uid": "rs-1" },
                })),
                cx,
            );
            panel.test_watch_controlled(Some(pods), Vec::new(), cx);
        })
    });
    assert_eq!(
        controls(&panel, &mut vcx),
        vec![ObjectRef::core("Pod", "staging", "web-1-a")]
    );
}

/// A Deployment panel controlling `web-1`, loaded, focused, with followed
/// references recorded.
fn deployment_with_replica_set(
    cx: &mut TestAppContext,
) -> (
    gpui_kit::VisualTestContext,
    std::rc::Rc<std::cell::RefCell<Vec<crate::ui::link::FollowReference>>>,
) {
    let followed = super::panel::init(cx);
    let (window, panel) = stub_panel(
        cx,
        target(deployments(), Some("staging"), "web"),
        vec![deployments(), replica_sets()],
    );
    let mut vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    let sets = vcx.update(|_, cx| {
        cx.new(|_| {
            synced_table(
                &replica_sets(),
                vec![owned("ReplicaSet", "web-1", &["deploy-1"])],
            )
        })
    });
    vcx.update(|window, cx| {
        panel.update(cx, |panel, cx| {
            panel.test_set_loaded(
                object(serde_json::json!({
                    "apiVersion": "apps/v1",
                    "kind": "Deployment",
                    "metadata": { "name": "web", "namespace": "staging", "uid": "deploy-1" },
                })),
                cx,
            );
            panel.test_watch_controlled(None, vec![(replica_sets(), sets)], cx);
        });
        panel.read(cx).focus_handle.clone().focus(window, cx);
    });
    vcx.run_until_parked();
    (vcx, followed)
}

fn web_1() -> ObjectRef {
    ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web-1")
}

#[gpui_kit::test]
async fn clicking_a_controlled_object_follows_it(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    let (mut vcx, followed) = deployment_with_replica_set(cx);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click(
            gpui_kit::ElementId::NamedInteger("Overview/Controls".into(), 0),
            cx,
        );
    });
    vcx.run_until_parked();
    assert_eq!(
        followed.borrow().last().map(|follow| follow.target.clone()),
        Some(web_1())
    );
}

#[gpui_kit::test]
async fn go_to_reaches_a_controlled_object_from_the_keyboard(cx: &mut TestAppContext) {
    let (mut vcx, followed) = deployment_with_replica_set(cx);
    // The picker lists the Namespace row, then the controlled ReplicaSet.
    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down enter");
    vcx.run_until_parked();
    assert_eq!(
        followed.borrow().last().map(|follow| follow.target.clone()),
        Some(web_1())
    );
}

#[test]
fn a_refused_kind_is_reported_as_unlistable() {
    let sets = replica_sets();
    let jobs = kind("batch", "v1", "Job", true);
    let mut refused = ObjectsTable::for_kind(&sets);
    refused.set_refused("forbidden".to_string());
    let listed = synced_table(&jobs, Vec::new());

    assert_eq!(
        unlistable_kinds([(&sets, &refused), (&jobs, &listed)]),
        vec!["replicasets".to_string()],
        "only the refused kind, by its plural"
    );
    assert!(unlistable_kinds([(&jobs, &listed)]).is_empty());
}

#[gpui_kit::test]
async fn controls_says_it_cant_list_a_refused_child_kind(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (window, panel) = stub_panel(
        cx,
        target(deployments(), Some("staging"), "web"),
        vec![deployments(), replica_sets()],
    );
    let mut vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    let sets = vcx.update(|_, cx| {
        cx.new(|_| {
            let mut table = ObjectsTable::for_kind(&replica_sets());
            table.set_refused("forbidden".to_string());
            table
        })
    });
    vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.test_set_loaded(
                object(serde_json::json!({
                    "apiVersion": "apps/v1",
                    "kind": "Deployment",
                    "metadata": { "name": "web", "namespace": "staging", "uid": "deploy-1" },
                })),
                cx,
            );
            panel.test_watch_controlled(None, vec![(replica_sets(), sets)], cx);
        })
    });

    let text = vcx.update(|_, cx| {
        let sections = panel.read(cx).sections(jiff::Timestamp::now());
        sections[0]
            .fields
            .iter()
            .find(|field| field.label == "Controls")
            .map(|field| field.value.text())
    });
    assert_eq!(text.as_deref(), Some("can't list replicasets"));
}
