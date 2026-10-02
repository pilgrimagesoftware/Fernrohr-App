//! References in the rendered panel: a followable one is a link that
//! dispatches `FollowReference` when clicked, one with no viewer is plain text
//! that does nothing. `resource-links` 2.2 and 3.2's click half.

use super::fixtures::rich_pod;
use super::panel::stub_panel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::icon::test_support::{assert_icon_leads, icon_bounds};
use crate::ui::link::FollowReference;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use std::cell::RefCell;
use std::rc::Rc;

/// Every `FollowReference` that reaches the app, in order. The panel is the
/// window's root, so nothing in the element tree handles the action and it
/// bubbles to the app-level handler registered here - the same place it would
/// reach `MainWindow` in the real app.
fn record_follows(cx: &mut TestAppContext) -> Rc<RefCell<Vec<FollowReference>>> {
    let followed = Rc::new(RefCell::new(Vec::new()));
    let sink = followed.clone();
    cx.update(|cx| {
        cx.on_action(move |action: &FollowReference, _cx| {
            sink.borrow_mut().push(action.clone());
        });
    });
    followed
}

fn reference_id(field: &'static str, index: u64) -> ElementId {
    ElementId::NamedInteger(field.into(), index)
}

#[gpui_kit::test]
async fn a_namespace_reference_is_a_link_and_an_owner_with_no_viewer_is_not(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let followed = record_follows(cx);
    let window = stub_panel(cx, ConnectionState::Connecting);
    window
        .update(cx, |panel, _window, cx| {
            panel.test_set_loaded(rich_pod(), cx)
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(reference_id("Namespace", 0), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        *followed.borrow(),
        vec![FollowReference {
            context_name: "kind-dev".into(),
            target: ObjectRef::cluster_scoped("", "Namespace", "staging"),
        }],
        "clicking the namespace follows it, from the panel's own context"
    );

    // The owner is a ReplicaSet, which this context's discovery (empty here)
    // doesn't report: it is on screen, and clicking it follows nothing.
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(reference_id("Controlled By", 0)).is_some());
        window.click(reference_id("Controlled By", 0), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        followed.borrow().len(),
        1,
        "a reference with no viewer is plain text"
    );
}

/// `resource-links`' "a kind gains a viewer": the same owner reference that
/// was plain text above becomes a link once the context's discovery reports
/// ReplicaSets - nothing in pod detail changes, only what `viewer_for` knows.
#[gpui_kit::test]
async fn an_owner_becomes_a_link_once_its_kind_is_discovered(cx: &mut TestAppContext) {
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use kube::core::GroupVersionKind;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let followed = record_follows(cx);
    let window = stub_panel(cx, ConnectionState::Connecting);
    window
        .update(cx, |panel, _window, cx| {
            panel.test_set_loaded(rich_pod(), cx);
            panel.test_set_kinds(
                vec![DiscoveredKind {
                    gvk: GroupVersionKind::gvk("apps", "v1", "ReplicaSet"),
                    plural: "replicasets".into(),
                    namespaced: true,
                }],
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(reference_id("Controlled By", 0), cx);
    })
    .unwrap();
    cx.run_until_parked();

    assert_eq!(
        *followed.borrow(),
        vec![FollowReference {
            context_name: "kind-dev".into(),
            target: ObjectRef::namespaced("apps", "ReplicaSet", "staging", "api-7d9f"),
        }]
    );
}

/// The panel as the app hosts it: inside a `Root`, under a view that draws the
/// dialog layer the "Go to…" picker opens in.
struct Host {
    panel: gpui_kit::Entity<crate::k8s::resource::pod_detail::panel::PodDetailPanel>,
}

impl gpui_kit::Render for Host {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{ParentElement as _, Styled as _};
        gpui_kit::div().size_full().child(self.panel.clone())
    }
}

/// 4.1 and 4.2 on the real panel: the `g` hint shows only once there is a
/// followable reference, and `g` then Enter follows it.
#[gpui_kit::test]
async fn g_opens_the_picker_on_a_pod_and_enter_follows_its_namespace(cx: &mut TestAppContext) {
    use crate::command::CommandRegistry;
    use crate::k8s::cluster::connection::ClusterConnection;
    use crate::k8s::resource::pod_detail::model::DetailView;
    use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
    use crate::keymap::{self, KeymapConfig};
    use crate::ui::nav::{NavTarget, PodRef};
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::VisualTestContext;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        // Settle the picker dialog's entrance on its first frame - see
        // `ui::link::tests::harness`.
        cx.set_reduce_motion(true);
        let mut registry = CommandRegistry::new();
        crate::ui::link::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let followed = record_follows(cx);
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let mut panel = None;
    let window = cx.add_window(|window, cx| {
        let pod = PodRef {
            namespace: "staging".into(),
            name: "api-7d9f-ftg5t".into(),
        };
        let scope = PanelScope::new(
            NavTarget::pod("staging", "api-7d9f-ftg5t"),
            "kind-dev".into(),
        );
        let built = cx.new(|cx| {
            PodDetailPanel::with_connection(pod, scope, DetailView::Structured, connection, cx)
        });
        panel = Some(built.clone());
        let host = cx.new(|_| Host { panel: built });
        gpui_kit::component::Root::new(host, window, cx)
    });
    let panel = panel.expect("the window built its panel");
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
            panel.update(cx, |panel, cx| panel.test_set_loaded(rich_pod(), cx));
            panel.read(cx).focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    assert!(hint_shown(&mut vcx), "the namespace is followable");

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    assert_eq!(
        *followed.borrow(),
        vec![FollowReference {
            context_name: "kind-dev".into(),
            target: ObjectRef::cluster_scoped("", "Namespace", "staging"),
        }]
    );
}

/// `resource-kind-icons` 3.3: every reference leads with its kind's icon - a
/// followable link and plain text alike.
#[gpui_kit::test]
async fn every_reference_leads_with_its_kinds_icon(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx, ConnectionState::Connecting);
    window
        .update(cx, |panel, _window, cx| {
            panel.test_set_loaded(rich_pod(), cx)
        })
        .unwrap();
    let mut vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    assert_icon_leads(
        &mut vcx,
        "kind-icon Namespace-0 Namespace".into(),
        reference_id("Namespace", 0),
    );
    // The selector names the icon drawn, so another kind's isn't found.
    assert!(icon_bounds(&mut vcx, "kind-icon Namespace-0 Pod".into()).is_none());
    // Not followable here (no ReplicaSet viewer is discovered), still iconned.
    assert_icon_leads(
        &mut vcx,
        "kind-icon Controlled By-0 ReplicaSet".into(),
        reference_id("Controlled By", 0),
    );
}
