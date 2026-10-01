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
