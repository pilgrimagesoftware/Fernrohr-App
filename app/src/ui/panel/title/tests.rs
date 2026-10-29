//! Unit tests for `ui::panel::title`: the title, heading and namespace-picker
//! rules, and the heading's ellipsis/tooltip layout.

use super::{
    PanelScope, focus_underline, heading_name_box, is_truncated, item_heading, namespaces_offered,
    title, tooltip_lines,
};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::namespace_picker::label_for;
use crate::ui::nav::NavTarget;
use gpui_kit::{
    AvailableSpace, Context, IntoElement, Pixels, Render, StyledText, TestAppContext, TextLayout,
    Window, black, div, point, px, size,
};
use gpui_kit::{ParentElement as _, Styled as _};
use kube::core::GroupVersionKind;

/// Draws `name` in the heading's name box at `width`, returning the
/// layout the heading's tooltip consults.
fn draw_heading_name(cx: &mut TestAppContext, name: &str, width: Pixels) -> TextLayout {
    let cx = cx.add_empty_window();
    let text = StyledText::new(name.to_string());
    let layout = text.layout().clone();
    cx.draw(
        point(px(0.), px(0.)),
        size(AvailableSpace::Definite(width), AvailableSpace::MinContent),
        |_window, _cx| heading_name_box().w(width).child(text),
    );
    layout
}

/// The heading's tooltip appears only when the name did not fit: a narrow
/// box ellipsizes it, a wide one draws it whole.
#[gpui_kit::test]
fn heading_name_reports_truncation_only_when_ellipsized(cx: &mut TestAppContext) {
    let name = "gke-metrics-agent-bl7vc";

    let narrow = draw_heading_name(cx, name, px(40.));
    assert!(is_truncated(&narrow, name), "drawn as {:?}", narrow.text());

    let wide = draw_heading_name(cx, name, px(2000.));
    assert!(!is_truncated(&wide, name), "drawn as {:?}", wide.text());
}

/// In a heading too narrow for both, the name and the context share the
/// width: neither is squeezed to nothing, and the context stays inside the
/// heading rather than drawing over whatever sits beside it.
/// A view drawing only an item heading, 128px wide.
struct NarrowHeading;

impl Render for NarrowHeading {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(128.)).child(item_heading(
            "gke-metrics-agent-bl7vc".to_string(),
            Some("northbay-staging-us-west1".to_string()),
            black(),
        ))
    }
}

#[gpui_kit::test]
fn a_narrow_heading_keeps_name_and_context_inside_it(cx: &mut TestAppContext) {
    let width = px(128.);
    // A view rather than `draw`: the heading's text is stateful
    // (`InteractiveText`), and element state lives under a view.
    let (_view, cx) = cx.add_window_view(|_window, _cx| NarrowHeading);
    cx.run_until_parked();

    let name = cx.debug_bounds("item-heading-name").expect("name drawn");
    let context = cx
        .debug_bounds("item-heading-context")
        .expect("context drawn");
    assert!(name.size.width >= width / 4., "name squeezed out: {name:?}");
    assert!(
        context.size.width > px(0.),
        "context squeezed out: {context:?}"
    );
    assert!(
        context.right() <= width,
        "context overflows the heading: {context:?}"
    );
}

fn kind(kind: &str, namespaced: bool) -> NavTarget {
    NavTarget::Kind(DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", kind),
        plural: format!("{}s", kind.to_lowercase()),
        namespaced,
        verbs: Default::default(),
    })
}

fn scope(target: NavTarget, connections: usize) -> PanelScope {
    PanelScope {
        connection_count: connections,
        ..PanelScope::new(target, "kind-dev".to_string())
    }
}

/// Section 10.1: one connection means the cluster name is noise - every
/// panel in the window would repeat the only cluster there is.
#[test]
fn a_single_connection_leaves_the_cluster_out_of_the_title() {
    assert_eq!(title(&scope(kind("Pod", true), 1)), "Pods");
}

/// With several connections the title still leaves the cluster out; the tab's
/// tooltip and the panel's "Context:" line name it instead.
#[test]
fn several_connections_still_leave_the_cluster_out_of_the_title() {
    assert_eq!(title(&scope(kind("Pod", true), 2)), "Pods");
    assert_eq!(title(&scope(kind("Deployment", true), 3)), "Deployments");
}

fn grouped(group: &str, kind: &str, plural: &str) -> NavTarget {
    NavTarget::Kind(DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.to_string(),
        namespaced: true,
        verbs: Default::default(),
    })
}

/// `standard-resource-panels` 5.3: a custom resource list's tab and title bar
/// read the plural kind only, and its tooltip names the API group above the
/// context.
#[test]
fn a_custom_resource_list_titles_by_kind_with_its_group_in_the_tooltip() {
    let certificates = scope(grouped("cert-manager.io", "Certificate", "certificates"), 1);
    assert_eq!(title(&certificates), "Certificates");
    assert_eq!(
        tooltip_lines(&certificates),
        (
            Some("cert-manager.io/v1".into()),
            "Context: kind-dev".into()
        )
    );
}

/// #206: a built-in kind outside the core group titles by its plural alone
/// too - the Gateway API's HTTPRoutes as well as `apps`' Deployments - and its
/// tooltip gives its `group/version`.
#[test]
fn a_built_in_grouped_list_titles_by_kind_with_its_group_in_the_tooltip() {
    let routes = scope(
        grouped("gateway.networking.k8s.io", "HTTPRoute", "httproutes"),
        1,
    );
    assert_eq!(title(&routes), "HTTPRoutes");
    assert_eq!(
        tooltip_lines(&routes),
        (
            Some("gateway.networking.k8s.io/v1".into()),
            "Context: kind-dev".into()
        )
    );
    let deployments = scope(grouped("apps", "Deployment", "deployments"), 1);
    assert_eq!(title(&deployments), "Deployments");
    assert_eq!(
        tooltip_lines(&deployments),
        (Some("apps/v1".into()), "Context: kind-dev".into())
    );
}

/// A core kind's tooltip gives its version alone, as its `apiVersion` reads.
#[test]
fn a_core_lists_tooltip_gives_its_version() {
    let pods = scope(kind("Pod", true), 1);
    assert_eq!(title(&pods), "Pods");
    assert_eq!(
        tooltip_lines(&pods),
        (Some("v1".into()), "Context: kind-dev".into())
    );
}

fn discovered(target: &NavTarget) -> DiscoveredKind {
    match target {
        NavTarget::Kind(kind) => kind.clone(),
        _ => panic!("a kind"),
    }
}

/// #206: two groups' kinds of one name - core `Event` and `events.k8s.io`'s -
/// title apart: the grouped one adds its group, the core one keeps its bare
/// name. A kind whose name nothing else has stays unqualified.
#[test]
fn two_kinds_sharing_a_name_title_apart() {
    let core = grouped("", "Event", "events");
    let events_api = grouped("events.k8s.io", "Event", "events");
    let certificates = grouped("cert-manager.io", "Certificate", "certificates");
    let kinds = [
        discovered(&core),
        discovered(&events_api),
        discovered(&certificates),
    ];
    let shares = |kind: &DiscoveredKind| kind.shares_name_in(&kinds);
    assert_eq!(core.item_label_qualified(shares), "Events");
    assert_eq!(
        events_api.item_label_qualified(shares),
        "Events · events.k8s.io"
    );
    assert_eq!(certificates.item_label_qualified(shares), "Certificates");
}

/// An object of a kind sharing its name says which group's it is, too.
#[test]
fn an_object_of_a_shared_name_kind_titles_with_its_group() {
    let events_api = discovered(&grouped("events.k8s.io", "Event", "events"));
    let core = discovered(&grouped("", "Event", "events"));
    let object = NavTarget::Object(crate::ui::nav::ObjectTarget {
        kind: events_api.clone(),
        namespace: Some("shop".into()),
        name: "web.1".into(),
    });
    let kinds = [core, events_api];
    assert_eq!(
        object.item_label_qualified(|kind| kind.shares_name_in(&kinds)),
        "Event · events.k8s.io: web.1"
    );
    assert_eq!(object.item_label(), "Event: web.1", "unqualified alone");
}

/// Two versions of one kind aren't two kinds: nothing to tell apart.
#[test]
fn another_version_of_the_same_kind_is_no_clash() {
    let v1 = discovered(&grouped(
        "autoscaling",
        "HorizontalPodAutoscaler",
        "horizontalpodautoscalers",
    ));
    let mut v2 = v1.clone();
    v2.gvk.version = "v2".into();
    assert!(!v1.shares_name_in(&[v1.clone(), v2]));
}

/// Error text goes through markdown (the only selectable-text primitive), so
/// it must come out verbatim: escaped prose, and detail fenced so backticks and
/// backslashes survive.
#[test]
fn error_text_is_rendered_verbatim() {
    use super::error::{code_block, escape_markdown};
    assert_eq!(escape_markdown("continue_: None"), "continue\\_: None");
    assert_eq!(escape_markdown("a*b"), "a\\*b");
    assert_eq!(code_block(r#"pods \"x\""#), "```text\npods \\\"x\\\"\n```");
    assert_eq!(code_block("a ```` b"), "`````text\na ```` b\n`````");
}

/// An item heading names the context only while the window uses several.
#[test]
fn a_heading_names_the_context_only_for_several() {
    use super::heading_context;
    let pod = scope(NavTarget::pod("default", "web-1"), 1);
    assert_eq!(heading_context(&pod, 1), None);
    assert_eq!(heading_context(&pod, 2), Some("kind-dev".to_string()));
}

/// The Logs view is a target of its own and gets the same rule.
#[test]
fn the_logs_view_titles_by_the_same_rule() {
    assert_eq!(title(&scope(NavTarget::Logs, 1)), "Logs");
    assert_eq!(title(&scope(NavTarget::Logs, 2)), "Logs");
}

/// Section 2.2: a panel over one pod names the pod, so two pods' detail
/// panels are told apart in the dock's tabs rather than both reading "Pod".
/// The plural-for-a-list rule still holds next to it.
#[test]
fn a_pods_detail_panel_names_its_pod() {
    let pod_scope = scope(NavTarget::pod("default", "api-7d9f-ftg5t"), 1);
    assert_eq!(title(&pod_scope), "Pod: api-7d9f-ftg5t");

    let other = scope(NavTarget::pod("default", "web-1"), 1);
    assert_ne!(title(&pod_scope), title(&other), "two pods, two names");

    let with_cluster = scope(NavTarget::pod("default", "api-7d9f-ftg5t"), 2);
    assert_eq!(title(&with_cluster), "Pod: api-7d9f-ftg5t");
}

/// A pod is namespaced whatever discovery said about any kind, so a panel
/// over one is namespaced rather than silently scoping itself out of one.
#[test]
fn a_pod_detail_panel_is_namespaced() {
    assert!(scope(NavTarget::pod("kube-system", "coredns-1"), 1).is_namespaced());
}

/// Section 10.2: a namespaced kind gets a picker, a cluster-scoped kind
/// does not. This is the rule, read straight off discovery's own scope.
#[test]
fn the_namespace_picker_follows_the_kinds_scope() {
    assert!(scope(kind("Pod", true), 1).is_namespaced());
    assert!(!scope(kind("Node", false), 1).is_namespaced());
    assert!(!scope(kind("Namespace", false), 1).is_namespaced());
    // The log view is over one pod, so it is namespaced whatever the
    // discovery entry for a hypothetical cluster-scoped kind says.
    assert!(scope(NavTarget::Logs, 1).is_namespaced());
}

/// The picker's current value reads as the scope it represents, and the
/// default reads as "all namespaces" rather than as an empty control.
#[test]
fn the_picker_reads_its_current_scope() {
    let mut s = scope(kind("Pod", true), 1);
    assert_eq!(label_for(&s.namespaces), "All namespaces");
    s = s.scoped_to(vec!["staging".to_string()]);
    assert_eq!(label_for(&s.namespaces), "staging");
    s = s.scoped_to(vec!["staging".to_string(), "default".to_string()]);
    assert_eq!(label_for(&s.namespaces), "2 namespaces");
    s = s.scoped_to(Vec::new());
    assert_eq!(label_for(&s.namespaces), "All namespaces");
}

/// A window can only represent the scopes the picker offers, so the default
/// it starts on has to be among them.
#[test]
fn the_default_scope_is_one_the_picker_offers() {
    assert!(
        namespaces_offered(&[]).contains(&None),
        "the scope a panel starts on must be selectable"
    );
}

/// The title bar and the panel key have to agree about scope, so scoping is
/// a change of scope and nothing else.
#[test]
fn scoping_changes_only_the_namespace() {
    let s = scope(kind("Pod", true), 2);
    let scoped = s.scoped_to(vec!["staging".to_string(), "default".to_string()]);
    assert_eq!(scoped.namespaces, ["default", "staging"]);
    assert_eq!(scoped.target, s.target);
    assert_eq!(scoped.context_name, s.context_name);
    assert_eq!(scoped.connection_count, s.connection_count);
}

/// The focused panel's title is underlined in the accent colour; every other
/// panel's underline is there but invisible, so labels don't shift as focus
/// moves.
#[test]
fn only_the_focused_panel_underlines_its_title() {
    let accent = gpui_kit::hsla(0.6, 0.9, 0.6, 1.);
    assert_eq!(focus_underline(true, accent), accent);
    assert_eq!(focus_underline(false, accent).a, 0.);
}

/// `open-panel-new-instance` 1.1: the first instance's title is as it was;
/// later ones end in their number, `instance + 1`.
#[test]
fn instances_after_the_first_are_numbered() {
    assert_eq!(super::numbered("Pod: web-1".into(), 0), "Pod: web-1");
    assert_eq!(super::numbered("Pod: web-1".into(), 1), "Pod: web-1 (2)");
    assert_eq!(super::numbered("Deployments".into(), 2), "Deployments (3)");
}

/// 1.1: a first instance saves no number and reads back as 0; a later one
/// round-trips, and a scope's instance is part of what it is.
#[test]
fn instances_save_and_read_back() {
    let first = scope(NavTarget::pods(), 1);
    let second = first.as_instance(1);
    assert_ne!(first, second);
    let saved = super::with_saved_scope(&first, serde_json::json!({ "context_name": "dev" }));
    assert!(saved.get("instance").is_none());
    assert_eq!(super::saved_instance(&saved), 0);
    let saved = super::with_saved_scope(&second, serde_json::json!({ "context_name": "dev" }));
    assert_eq!(super::saved_instance(&saved), 1);
    assert_eq!(
        super::saved_instance(&serde_json::json!({ "instance": "two" })),
        0
    );
}

/// `title_in` reads the context's discovery: once it holds both `Event`s, the
/// grouped one's tab says its group; before discovery, nothing qualifies it.
#[gpui_kit::test]
async fn title_in_qualifies_from_the_contexts_discovery(cx: &mut gpui_kit::TestAppContext) {
    use super::title_in;
    use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
    let events_api = grouped("events.k8s.io", "Event", "events");
    let events = scope(events_api.clone(), 1);
    assert_eq!(cx.update(|cx| title_in(&events, cx)), "Events");
    cx.update(|cx| {
        DiscoveryRegistry::insert_test(
            cx,
            "kind-dev",
            vec![
                discovered(&grouped("", "Event", "events")),
                discovered(&events_api),
            ],
        )
    });
    assert_eq!(
        cx.update(|cx| title_in(&events, cx)),
        "Events · events.k8s.io"
    );
}
