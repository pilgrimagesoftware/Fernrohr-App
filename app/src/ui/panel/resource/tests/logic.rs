//! GPUI-free coverage for section 1's category lookup, section 2's grouping,
//! and section 3's filter matching/visibility - no window needed, which is
//! what keeps it fast and exhaustive. `super::kind` (this module's parent,
//! `resource::tests`) builds the fixtures.

use super::kind;
use crate::k8s::cluster::discovery::DiscoveredKind;

type Category = super::super::category::Category;

fn category_for(kind: &crate::k8s::cluster::discovery::DiscoveredKind) -> Category {
    Category::for_gvk(&kind.gvk.group, &kind.plural)
}

/// Section 1.1: the six core-group kinds design.md names land in six
/// different sections, and an unknown group/plural pair falls back to
/// Custom Resources rather than being dropped or crashing the lookup.
#[test]
fn core_group_kinds_land_in_six_different_categories() {
    let pod = category_for(&kind("", "Pod"));
    let service = category_for(&kind("", "Service"));
    let config_map = category_for(&kind("", "ConfigMap"));
    let pvc = category_for(&kind("", "PersistentVolumeClaim"));
    let service_account = category_for(&kind("", "ServiceAccount"));
    let namespace = category_for(&kind("", "Namespace"));

    let mut seen = std::collections::HashSet::new();
    for category in [pod, service, config_map, pvc, service_account, namespace] {
        assert!(
            seen.insert(format!("{category}")),
            "expected six distinct categories, got a repeat: {category}"
        );
    }
}

#[test]
fn an_unmatched_group_and_plural_falls_back_to_custom_resources() {
    let unknown = Category::for_gvk("ferns.example.com", "ferns");
    assert_eq!(unknown.to_string(), "Custom Resources");
}

/// Section 1.2: every section is reachable by at least one built-in kind -
/// picking one representative per section and asserting its exact category
/// (not just "not Custom Resources") is what would catch a built-in kind
/// silently falling through to the fallback bucket, the failure the task
/// names explicitly.
#[test]
fn every_section_is_reachable_by_a_built_in_kind() {
    let cases: &[(&str, &str, &str)] = &[
        ("apps", "deployments", "Workloads"),
        ("", "resourcequotas", "Config"),
        ("networking.k8s.io", "ingresses", "Network"),
        ("storage.k8s.io", "storageclasses", "Storage"),
        ("", "namespaces", "Cluster"),
        ("scheduling.k8s.io", "priorityclasses", "Cluster"),
        (
            "rbac.authorization.k8s.io",
            "clusterroles",
            "Access Control",
        ),
    ];
    for (group, plural, expected) in cases {
        assert_eq!(
            Category::for_gvk(group, plural).to_string(),
            *expected,
            "{group}/{plural} should be {expected}"
        );
    }
}

/// Section 2.1: kinds spanning several sections, plus a duplicate-free CRD,
/// partition into sections in the fixed order with no kind lost.
#[test]
fn group_kinds_orders_sections_and_loses_no_kind() {
    let kinds = vec![
        kind("apps", "Deployment"),
        kind("", "Pod"),
        kind("", "Service"),
        kind("ferns.example.com", "Fern"),
    ];

    let sections = super::super::section::group_kinds(&kinds);

    let order: Vec<String> = sections
        .iter()
        .map(|section| section.category.to_string())
        .collect();
    assert_eq!(order, vec!["Workloads", "Network", "Custom Resources"]);

    let total: usize = sections.iter().map(|section| section.kinds.len()).sum();
    assert_eq!(total, kinds.len(), "no kind lost across sections");
}

/// An empty discovery result partitions into no sections at all - nothing to
/// render, rather than a spurious empty header for every category.
#[test]
fn grouping_no_kinds_yields_no_sections() {
    let kinds: Vec<DiscoveredKind> = Vec::new();
    assert!(super::super::section::group_kinds(&kinds).is_empty());
}

/// Section 3.1: matching checks the row's label, kind, plural and API group -
/// including a group that appears in neither the label nor the kind name.
#[test]
fn matches_filter_checks_label_kind_plural_and_group() {
    let cron_job = kind("batch", "CronJob");

    assert!(super::super::section::matches_filter(&cron_job, "cronjob"));
    assert!(super::super::section::matches_filter(
        &cron_job,
        "CronJob · batch"
    ));
    assert!(super::super::section::matches_filter(&cron_job, "cronjobs"));
    assert!(
        super::super::section::matches_filter(&cron_job, "batch"),
        "the group alone should match, even though it's in neither the label nor the kind"
    );
    assert!(!super::super::section::matches_filter(&cron_job, "ingress"));
    assert!(
        super::super::section::matches_filter(&cron_job, ""),
        "an empty filter matches everything"
    );
}

/// Section 3.2/3.3: a collapsed section with a match renders expanded and
/// keeps its match; a section with none is dropped entirely; the stored
/// collapse set itself is left untouched by any of this.
#[test]
fn visible_sections_expands_matches_and_hides_the_rest_without_mutating_collapse() {
    let ingress = DiscoveredKind {
        plural: "ingresses".to_string(),
        ..kind("networking.k8s.io", "Ingress")
    };
    let kinds = vec![kind("apps", "Deployment"), ingress];
    let mut collapsed = std::collections::HashSet::new();
    collapsed.insert(Category::Workloads);

    let filtered = super::super::section::visible_sections(&kinds, &collapsed, "ingress");
    assert_eq!(filtered.len(), 1, "only the matching section remains");
    assert_eq!(filtered[0].category.to_string(), "Network");
    assert!(
        filtered[0].expanded,
        "a match forces its section expanded regardless of collapse state"
    );

    assert!(
        collapsed.contains(&Category::Workloads),
        "filtering must not mutate the stored collapse state"
    );

    let no_match = super::super::section::visible_sections(&kinds, &collapsed, "nonesuch");
    assert!(
        no_match.is_empty(),
        "no sections render when nothing matches"
    );

    let cleared = super::super::section::visible_sections(&kinds, &collapsed, "");
    assert!(
        cleared
            .iter()
            .find(|section| section.category.to_string() == "Workloads")
            .is_some_and(|section| !section.expanded),
        "clearing the filter restores Workloads to its stored collapsed state"
    );
}
