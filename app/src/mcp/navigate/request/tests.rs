use super::*;
use crate::mcp::test_support::discovered;

fn label(value: &str) -> Option<LabelName> {
    Some(LabelName::parse("namespace", value).unwrap())
}

fn object(value: &str) -> Option<ObjectName> {
    Some(ObjectName::parse("name", value).unwrap())
}

#[test]
fn a_list_is_scoped_to_the_namespace_given_or_left_to_the_default() {
    let deployments = discovered("apps", "v1", "Deployment", "deployments", true);
    let (target, namespaces) = panel_target(&deployments, label("team-a"), None).unwrap();
    assert_eq!(target, NavTarget::Kind(deployments.clone()));
    assert_eq!(namespaces, ["team-a"]);
    let (_, namespaces) = panel_target(&deployments, None, None).unwrap();
    assert!(namespaces.is_empty());
}

#[test]
fn a_named_object_opens_its_detail_panel() {
    let (target, namespaces) =
        panel_target(&DiscoveredKind::pods(), label("team-a"), object("api-0")).unwrap();
    assert_eq!(target, NavTarget::pod("team-a", "api-0"));
    assert!(namespaces.is_empty());

    let nodes = discovered("", "v1", "Node", "nodes", false);
    let (target, _) = panel_target(&nodes, None, object("node-1")).unwrap();
    assert_eq!(
        target,
        NavTarget::Object(ObjectTarget {
            kind: nodes,
            namespace: None,
            name: "node-1".into(),
        })
    );
}

#[test]
fn a_scope_that_does_not_fit_the_kind_is_refused() {
    let nodes = discovered("", "v1", "Node", "nodes", false);
    assert!(matches!(
        panel_target(&nodes, label("team-a"), None),
        Err(ToolError::InvalidArguments { .. })
    ));
    assert!(matches!(
        panel_target(&DiscoveredKind::pods(), None, object("api-0")),
        Err(ToolError::InvalidArguments { .. })
    ));
}
