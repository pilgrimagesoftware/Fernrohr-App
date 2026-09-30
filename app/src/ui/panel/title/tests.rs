//! Unit tests for `ui::panel::title`: the title, heading and namespace-picker
//! rules, and the heading's ellipsis/tooltip layout.

use super::{
    PanelScope, heading_name_box, is_truncated, item_heading, label_for, namespaces_offered, title,
};
use crate::k8s::cluster::discovery::DiscoveredKind;
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
            Some("carefulcrab-staging-us-west1".to_string()),
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

/// Error text goes through markdown (the only selectable-text primitive), so
/// it must come out verbatim: escaped prose, and detail fenced so backticks and
/// backslashes survive.
#[test]
fn error_text_is_rendered_verbatim() {
    use super::{code_block, escape_markdown};
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
