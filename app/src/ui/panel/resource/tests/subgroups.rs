//! `custom-resource-grouping` §1: the Custom Resources section's kinds bucketed
//! by API group, and the per-window collapsed-subgroup set.

use super::super::category::Category;
use super::super::section::{custom_subgroups, group_kinds};
use super::{kind, stub_panel};
use crate::k8s::cluster::discovery::DiscoveredKind;
use gpui_kit::TestAppContext;

/// A cluster's kinds as `discover_kinds` returns them: by group then kind,
/// core first. Built-ins the taxonomy names (Pod, Deployment, Service) land in
/// their own sections; everything else - an unnamed core kind and three CRD
/// groups - lands in Custom Resources.
fn discovered() -> Vec<DiscoveredKind> {
    vec![
        kind("", "Pod"),
        kind("", "Service"),
        kind("", "Widget"),
        kind("apps", "Deployment"),
        kind("argoproj.io", "Application"),
        kind("argoproj.io", "Rollout"),
        kind("cert-manager.io", "Certificate"),
        kind("cert-manager.io", "Issuer"),
        kind("networking.istio.io", "VirtualService"),
    ]
}

fn custom_resources() -> Vec<DiscoveredKind> {
    group_kinds(&discovered())
        .into_iter()
        .find(|section| section.category == Category::CustomResources)
        .expect("CRDs fill the Custom Resources section")
        .kinds
}

/// Subgroups come core first, then alphabetically by group, and each holds
/// exactly its own group's kinds, in discovery order.
#[test]
fn custom_resources_subgroup_by_api_group_core_first() {
    let subgroups = custom_subgroups(&custom_resources());
    let summary: Vec<(&str, Vec<&str>)> = subgroups
        .iter()
        .map(|subgroup| {
            (
                subgroup.group.as_str(),
                subgroup
                    .kinds
                    .iter()
                    .map(|kind| kind.gvk.kind.as_str())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("", vec!["Widget"]),
            ("argoproj.io", vec!["Application", "Rollout"]),
            ("cert-manager.io", vec!["Certificate", "Issuer"]),
            ("networking.istio.io", vec!["VirtualService"]),
        ]
    );
}

/// Bucketing loses and repeats nothing: the subgroups' kinds are exactly the
/// section's kinds.
#[test]
fn subgrouping_keeps_every_kind_once() {
    let kinds = custom_resources();
    let mut regrouped: Vec<DiscoveredKind> = custom_subgroups(&kinds)
        .into_iter()
        .flat_map(|subgroup| subgroup.kinds)
        .collect();
    let mut expected = kinds.clone();
    let key = |kind: &DiscoveredKind| (kind.gvk.group.clone(), kind.gvk.kind.clone());
    regrouped.sort_by_key(key);
    expected.sort_by_key(key);
    assert_eq!(regrouped, expected);
}

/// The order doesn't depend on the input already being sorted: groups that
/// arrive out of order still come out core first, then alphabetical, while
/// each group's kinds keep their arrival order.
#[test]
fn subgroup_order_does_not_rely_on_sorted_input() {
    let kinds = vec![
        kind("zeta.example.com", "Zed"),
        kind("alpha.example.com", "Beta"),
        kind("", "Widget"),
        kind("alpha.example.com", "Alpha"),
    ];
    let groups: Vec<(String, Vec<String>)> = custom_subgroups(&kinds)
        .into_iter()
        .map(|subgroup| {
            (
                subgroup.group,
                subgroup
                    .kinds
                    .into_iter()
                    .map(|kind| kind.gvk.kind)
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        vec![
            ("".to_string(), vec!["Widget".to_string()]),
            (
                "alpha.example.com".to_string(),
                vec!["Beta".to_string(), "Alpha".to_string()]
            ),
            ("zeta.example.com".to_string(), vec!["Zed".to_string()]),
        ]
    );
}

/// A fresh panel - a new window on a connection - starts with every subgroup
/// expanded: nothing in the collapsed-subgroup set.
#[gpui_kit::test]
async fn a_fresh_panel_has_every_subgroup_expanded(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    window
        .update(cx, |panel, _window, _cx| {
            assert!(panel.collapsed_subgroups.is_empty());
        })
        .unwrap();
}

/// A panel showing `discovered()`, drawn in a window.
fn loaded_panel(
    cx: &mut TestAppContext,
) -> (
    gpui_kit::WindowHandle<super::super::ResourcePanel>,
    gpui_kit::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    window
        .update(cx, |panel, _window, cx| {
            panel.state = super::super::ResourceState::Loaded(discovered());
            cx.notify();
        })
        .unwrap();
    let vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    (window, vcx)
}

fn drawn(cx: &mut gpui_kit::VisualTestContext, selector: &str) -> bool {
    let selector: &'static str = selector.to_string().leak();
    cx.debug_bounds(selector).is_some()
}

/// 2.1: Custom Resources draws a header per API group with its kinds under
/// it; collapsing one subgroup hides only its rows - its header stays, and
/// the other subgroups' rows stay.
#[gpui_kit::test]
async fn collapsing_one_subgroup_leaves_the_others_visible(cx: &mut TestAppContext) {
    let (window, mut vcx) = loaded_panel(cx);
    for group in [
        "core",
        "argoproj.io",
        "cert-manager.io",
        "networking.istio.io",
    ] {
        assert!(
            drawn(&mut vcx, &format!("resource-subgroup-{group}")),
            "{group} header"
        );
    }
    assert!(
        drawn(&mut vcx, "resource-row-subgroup-argoproj.io-1"),
        "Rollout row"
    );

    window
        .update(&mut vcx, |panel, _window, cx| {
            panel.collapsed_subgroups.insert("argoproj.io".into());
            cx.notify();
        })
        .unwrap();
    vcx.run_until_parked();

    assert!(
        drawn(&mut vcx, "resource-subgroup-argoproj.io"),
        "its header stays"
    );
    assert!(
        !drawn(&mut vcx, "resource-row-subgroup-argoproj.io-0"),
        "its rows hide"
    );
    assert!(
        drawn(&mut vcx, "resource-row-subgroup-cert-manager.io-0"),
        "others stay"
    );
    assert!(
        drawn(&mut vcx, "resource-row-subgroup-core-0"),
        "core stays"
    );
}

/// 2.2: a click on a subgroup's header toggles it - both what's drawn and the
/// collapsed set - and a second click restores it.
#[gpui_kit::test]
async fn clicking_a_subgroup_header_toggles_it(cx: &mut TestAppContext) {
    let (window, mut vcx) = loaded_panel(cx);
    let collapsed = |vcx: &mut gpui_kit::VisualTestContext| {
        window
            .update(vcx, |panel, _window, _cx| panel.collapsed_subgroups.clone())
            .unwrap()
    };
    let click_header = |vcx: &mut gpui_kit::VisualTestContext| {
        let bounds = vcx
            .debug_bounds("resource-subgroup-cert-manager.io")
            .expect("the header is drawn");
        vcx.simulate_click(bounds.center(), gpui_kit::Modifiers::none());
        vcx.run_until_parked();
    };

    click_header(&mut vcx);
    assert!(collapsed(&mut vcx).contains("cert-manager.io"));
    assert!(!drawn(&mut vcx, "resource-row-subgroup-cert-manager.io-0"));

    click_header(&mut vcx);
    assert!(collapsed(&mut vcx).is_empty());
    assert!(drawn(&mut vcx, "resource-row-subgroup-cert-manager.io-0"));
}

/// A collapsed subgroup's kinds can't be reached from the keyboard either:
/// they leave the order Up/Down step through, the same moment they hide.
#[gpui_kit::test]
async fn a_collapsed_subgroups_kinds_leave_the_keyboard_order(cx: &mut TestAppContext) {
    let (window, mut vcx) = loaded_panel(cx);
    let reachable = |vcx: &mut gpui_kit::VisualTestContext| {
        window
            .update(vcx, |panel, _window, cx| {
                super::super::keyboard::visible_kinds(&panel.visible_sections(cx))
                    .into_iter()
                    .map(|kind| kind.gvk.kind.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    assert!(reachable(&mut vcx).contains(&"Rollout".to_string()));
    window
        .update(&mut vcx, |panel, _window, cx| {
            panel.collapsed_subgroups.insert("argoproj.io".into());
            cx.notify();
        })
        .unwrap();
    let after = reachable(&mut vcx);
    assert!(!after.contains(&"Rollout".to_string()));
    assert!(!after.contains(&"Application".to_string()));
    assert!(
        after.contains(&"Certificate".to_string()),
        "other groups stay reachable"
    );
}

/// Three CRD groups, two of which hold a kind named like `widget`.
fn three_groups() -> Vec<DiscoveredKind> {
    vec![
        kind("alpha.example.com", "Widget"),
        kind("beta.example.com", "WidgetPolicy"),
        kind("gamma.example.com", "Gadget"),
    ]
}

/// The Custom Resources section's subgroups, as `visible_sections` draws
/// them, for `filter` and the collapsed-subgroup set.
fn subgroups_for(
    kinds: &[DiscoveredKind],
    collapsed_subgroups: &std::collections::HashSet<String>,
    filter: &str,
) -> Vec<super::super::section::VisibleSubgroup> {
    super::super::section::visible_sections(kinds, &Default::default(), collapsed_subgroups, filter)
        .into_iter()
        .find(|section| section.category == Category::CustomResources)
        .map(|section| section.subgroups)
        .unwrap_or_default()
}

/// 3.1: the filter is matched per kind, and a subgroup shows only if one of
/// its kinds matched - a filter matching kinds in two of three subgroups
/// yields exactly those two.
#[test]
fn a_filter_shows_only_subgroups_with_a_match() {
    let shown: Vec<String> = subgroups_for(&three_groups(), &Default::default(), "widget")
        .into_iter()
        .map(|subgroup| subgroup.group)
        .collect();
    assert_eq!(shown, vec!["alpha.example.com", "beta.example.com"]);
}
