//! GPUI-free coverage for section 1's category lookup - no window needed,
//! which is what keeps it fast and exhaustive. `super::kind` (this module's
//! parent, `resource::tests`) builds the fixtures.

use super::kind;

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
