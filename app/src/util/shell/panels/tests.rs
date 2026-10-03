// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::panel_title::PanelScope;
use crate::util::shell::test_support::*;
use crate::util::shell::{NavTarget, PanelKey, WindowMode};
use gpui_kit::TestAppContext;

/// Section 5.1/5.4: the pod-detail request lands on the same `open_target`
/// every other panel uses, so a pod gets a panel of its own and
/// re-requesting it focuses rather than duplicates. What makes two pods
/// two panels is the pod identity now inside the key, not a second dedup
/// rule.
#[gpui_kit::test]
async fn a_pod_detail_panel_is_keyed_by_which_pod(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    let session_before = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

    window
        .update(cx, |main_window, window, cx| {
            main_window.open_target(NavTarget::pod("prod", "web-1"), window, cx);
            // The same pod again: focused, not opened twice.
            main_window.open_target(NavTarget::pod("prod", "web-1"), window, cx);
            // A different pod is a genuinely new panel.
            main_window.open_target(NavTarget::pod("prod", "web-2"), window, cx);

            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let pods: Vec<_> = open_panels
                .iter()
                .filter(|open| matches!(open.key.target, NavTarget::Pod(_)))
                .map(|open| open.key.target.clone())
                .collect();
            assert_eq!(
                pods,
                vec![
                    NavTarget::pod("prod", "web-1"),
                    NavTarget::pod("prod", "web-2"),
                ],
                "one panel per pod, however many times each is requested"
            );
        })
        .unwrap();
    cx.run_until_parked();

    let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
    assert_eq!(
        session_before, session_after,
        "the detail panel reads the window's existing connection"
    );
}

#[test]
fn restored_panel_keys_preserve_kind_context_and_namespace() {
    use gpui_kit::component::dock::{PanelInfo, PanelState};

    let state = PanelState {
        panel_name: "Pods".to_string(),
        children: Vec::new(),
        info: PanelInfo::Panel(serde_json::json!({
            "context_name": "kind-dev",
            "namespaces": ["kube-system"],
        })),
    };

    let keys = super::restored_panel_keys(&state);

    assert_eq!(keys.len(), 1);
    let key = keys[0].as_ref().expect("a Pods panel is keyed");
    assert_eq!(key.target, NavTarget::pods());
    assert_eq!(key.context_name, "kind-dev");
    assert_eq!(key.namespaces, vec!["kube-system"]);
}

/// `restore-no-panic`: a panel the window can't key still takes its slot, so the
/// keys after it stay paired with their own panels.
#[test]
fn an_unreadable_panel_keeps_its_slot_among_the_keys() {
    use gpui_kit::component::dock::{PanelInfo, PanelState};

    let panel = |name: &str, info: serde_json::Value| PanelState {
        panel_name: name.to_string(),
        children: Vec::new(),
        info: PanelInfo::Panel(info),
    };
    let tabs = PanelState {
        panel_name: "TabPanel".to_string(),
        children: vec![
            panel("Pods", serde_json::json!({})),
            panel("Logs", serde_json::json!({ "context_name": "kind-dev" })),
        ],
        info: PanelInfo::Tabs { active_index: 0 },
    };
    let state = PanelState {
        panel_name: "StackPanel".to_string(),
        children: vec![tabs],
        info: PanelInfo::Stack {
            sizes: Vec::new(),
            axis: 0,
        },
    };

    let keys = super::restored_panel_keys(&state);

    assert_eq!(keys.len(), 2, "one slot per restored panel");
    assert!(keys[0].is_none(), "the Pods panel names no cluster");
    assert_eq!(
        keys[1].as_ref().map(|key| key.target.clone()),
        Some(NavTarget::Logs),
        "the Logs panel keeps its own key"
    );
}

/// `standard-resource-panels` 1.6: a restored list panel - and a placeholder from an
/// older layout - is keyed by its kind, context and namespaces, so opening the same
/// kind again focuses it rather than adding a second.
#[test]
fn restored_list_panels_are_keyed_by_their_kind() {
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use gpui_kit::component::dock::{PanelInfo, PanelState};
    use kube::core::GroupVersionKind;

    let services = DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    for panel_name in ["ObjectList", "Resource"] {
        let state = PanelState {
            panel_name: panel_name.to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": "kind-dev",
                "namespaces": ["staging"],
                "group": "", "version": "v1", "kind": "Service",
                "plural": "services", "namespaced": true,
                "columns": [],
            })),
        };
        let keys = super::restored_panel_keys(&state);
        assert_eq!(keys.len(), 1, "{panel_name}");
        let key = keys[0].as_ref().expect("a list panel is keyed");
        assert_eq!(
            key.target,
            NavTarget::Kind(services.clone()),
            "{panel_name}"
        );
        assert_eq!(key.context_name, "kind-dev");
        assert_eq!(key.namespaces, vec!["staging"]);
    }
}

/// `PanelKey`'s dedup has to tell two contexts' panels over the same target
/// apart, or a pod (or Logs) opened on one context would focus the other
/// context's panel instead of opening its own - the structural half of
/// `show_logs_and_pod_detail_use_the_selected_pods_context_not_the_active_one`'s
/// end-to-end proof.
#[test]
fn panel_key_distinguishes_two_contexts_over_the_same_target() {
    let pods_a = PanelKey::from(&PanelScope::new(NavTarget::pods(), "a".to_string()));
    let pods_b = PanelKey::from(&PanelScope::new(NavTarget::pods(), "b".to_string()));
    assert_ne!(
        pods_a, pods_b,
        "the same target on two contexts must be two different keys"
    );

    let pod_a = PanelKey::from(&PanelScope::new(
        NavTarget::pod("default", "web-1"),
        "a".to_string(),
    ));
    let pod_b = PanelKey::from(&PanelScope::new(
        NavTarget::pod("default", "web-1"),
        "b".to_string(),
    ));
    assert_ne!(
        pod_a, pod_b,
        "the same pod's detail panel on two contexts must be two different keys"
    );
}
