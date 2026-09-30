//! The object panel in a window (`resource-links` 5.3-5.4): links that follow
//! only when their kind is discovered, `y` and `g` by real keystrokes, the `g`
//! hint, the not-found state, and dock restore.

use super::fixtures::{deployments, nodes, owned_replica_set, replica_sets, stub_panel, target};
use crate::command::CommandRegistry;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::restore::{dump_target, target_from_state};
use crate::k8s::resource::pod_detail::DetailView;
use crate::keymap::{self, KeymapConfig};
use crate::ui::link::FollowReference;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext, VisualTestContext};
use std::cell::RefCell;
use std::rc::Rc;

/// Initializes the app with the registry's real bindings for this panel and
/// `links.go_to`, and records every `FollowReference` that reaches the app.
fn init(cx: &mut TestAppContext) -> Rc<RefCell<Vec<FollowReference>>> {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        crate::ui::link::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let followed = Rc::new(RefCell::new(Vec::new()));
    let sink = followed.clone();
    cx.update(|cx| {
        cx.on_action(move |action: &FollowReference, _cx| sink.borrow_mut().push(action.clone()));
    });
    followed
}

fn owner_link() -> ElementId {
    ElementId::NamedInteger("Overview/Controlled By".into(), 0)
}

/// 5.3: the ReplicaSet's Deployment owner is a link once the cluster reports
/// Deployments, and clicking it follows it from the panel's context.
#[gpui_kit::test]
async fn a_discovered_owner_is_a_link_that_follows(cx: &mut TestAppContext) {
    let followed = init(cx);
    let (window, panel) = stub_panel(
        cx,
        target(replica_sets(), Some("staging"), "web-7d9f"),
        vec![replica_sets(), deployments()],
    );
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(owned_replica_set(), cx)
            })
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(owner_link(), cx);
    })
    .unwrap();
    cx.run_until_parked();

    assert_eq!(
        *followed.borrow(),
        vec![FollowReference {
            context_name: "kind-dev".into(),
            target: ObjectRef::namespaced("apps", "Deployment", "staging", "web"),
        }]
    );
}

/// The same owner, in a cluster whose discovery doesn't report Deployments,
/// is plain text.
#[gpui_kit::test]
async fn an_undiscovered_owner_is_plain_text(cx: &mut TestAppContext) {
    let followed = init(cx);
    let (window, panel) = stub_panel(
        cx,
        target(replica_sets(), Some("staging"), "web-7d9f"),
        vec![replica_sets()],
    );
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(owned_replica_set(), cx)
            })
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(owner_link(), cx);
    })
    .unwrap();
    cx.run_until_parked();

    assert!(followed.borrow().is_empty());
}

/// 5.3: `y` toggles to the YAML and back, by keystroke.
#[gpui_kit::test]
async fn y_toggles_the_yaml_view(cx: &mut TestAppContext) {
    init(cx);
    let (window, panel) = stub_panel(
        cx,
        target(replica_sets(), Some("staging"), "web-7d9f"),
        vec![replica_sets()],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(owned_replica_set(), cx)
            });
            panel.read(cx).focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();

    let view = |vcx: &mut VisualTestContext| {
        window
            .update(vcx, |_, _, cx| panel.read(cx).view())
            .unwrap()
    };
    assert_eq!(view(&mut vcx), DetailView::Structured);
    vcx.simulate_keystrokes("y");
    vcx.run_until_parked();
    assert_eq!(view(&mut vcx), DetailView::Yaml);
    vcx.simulate_keystrokes("y");
    vcx.run_until_parked();
    assert_eq!(view(&mut vcx), DetailView::Structured);
}

/// 5.3: the `g` hint shows once there is something to follow, and `g` then
/// Enter follows the first followable reference.
#[gpui_kit::test]
async fn g_then_enter_follows_the_first_reference(cx: &mut TestAppContext) {
    let followed = init(cx);
    let (window, panel) = stub_panel(
        cx,
        target(replica_sets(), Some("staging"), "web-7d9f"),
        vec![replica_sets(), deployments()],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let hint_shown = |vcx: &mut VisualTestContext| {
        vcx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.try_find("go-to-hint").is_some()
        })
        .unwrap()
    };
    assert!(!hint_shown(&mut vcx), "nothing loaded, nothing to go to");

    window
        .update(&mut vcx, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(owned_replica_set(), cx)
            });
            panel.read(cx).focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    assert!(hint_shown(&mut vcx));

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    assert_eq!(
        followed.borrow().last().map(|follow| follow.target.clone()),
        Some(ObjectRef::cluster_scoped("", "Namespace", "staging")),
        "the Namespace row comes first"
    );
}

/// The panel says its object doesn't exist, rather than erroring.
#[gpui_kit::test]
async fn a_missing_object_says_it_does_not_exist(cx: &mut TestAppContext) {
    init(cx);
    let (window, panel) = stub_panel(cx, target(nodes(), None, "gone"), vec![nodes()]);
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| panel.test_set_not_found(cx))
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("object-not-found").is_some());
    })
    .unwrap();
}

/// 5.4: what a panel saves is what restoring it reads back - including a
/// cluster-scoped object, which restores without a namespace.
#[test]
fn a_saved_panel_restores_the_same_object() {
    for original in [
        target(replica_sets(), Some("staging"), "web-7d9f"),
        target(nodes(), None, "node-a"),
    ] {
        let state = dump_target(&original, "kind-dev");
        assert_eq!(state["context_name"], "kind-dev");
        assert_eq!(target_from_state(&state), Some(original));
    }
    assert_eq!(target_from_state(&serde_json::json!({})), None);
}
