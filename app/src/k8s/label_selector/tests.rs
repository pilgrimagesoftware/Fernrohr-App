use super::{matches, of_object, parse};
use kube::api::DynamicObject;
use kube::core::{Expression, Selector};
use std::collections::BTreeMap;

fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn every_requirement_form_parses() {
    let selector = parse(
        "app=web, tier==front,env!=dev,team in (a, b),zone notin (x),canary,!legacy,example.com/role=api",
    )
    .expect("a valid selector");
    let expected: Selector = [
        Expression::Equal("app".into(), "web".into()),
        Expression::Equal("tier".into(), "front".into()),
        Expression::NotEqual("env".into(), "dev".into()),
        Expression::In("team".into(), ["a".into(), "b".into()].into()),
        Expression::NotIn("zone".into(), ["x".into()].into()),
        Expression::Exists("canary".into()),
        Expression::DoesNotExist("legacy".into()),
        Expression::Equal("example.com/role".into(), "api".into()),
    ]
    .into_iter()
    .collect();
    assert_eq!(selector, expected);
}

#[test]
fn the_canonical_text_parses_back_to_the_same_selector() {
    let selector = parse("app=web,team in (b,a),!legacy").unwrap();
    assert_eq!(parse(&selector.to_string()).unwrap(), selector);
}

#[test]
fn blank_text_is_the_select_everything_selector() {
    assert!(parse("  ").unwrap().selects_all());
}

#[test]
fn mistakes_name_the_requirement_at_fault() {
    for (text, requirement) in [
        ("app=web,", ""),
        ("app=web,,tier=front", ""),
        ("team in (a,b", "team in (a,b"),
        ("team in a,b)", "team in a,b)"),
        ("team (a)", "team (a)"),
        ("team in ()", "team in ()"),
        ("=web", "=web"),
        ("app=we b", "app=we b"),
        ("ap p", "ap p"),
    ] {
        let error = parse(text).expect_err(text);
        assert_eq!(error.requirement, requirement, "{text}");
    }
}

#[test]
fn a_selector_matches_pods_by_their_labels() {
    let selector = parse("app=web,env!=dev,!legacy").unwrap();
    assert!(matches(&selector, Some(&labels(&[("app", "web")]))));
    assert!(matches(
        &selector,
        Some(&labels(&[("app", "web"), ("env", "prod")]))
    ));
    assert!(!matches(
        &selector,
        Some(&labels(&[("app", "web"), ("env", "dev")]))
    ));
    assert!(!matches(
        &selector,
        Some(&labels(&[("app", "web"), ("legacy", "")]))
    ));
    assert!(!matches(&selector, None));
}

fn object(spec: serde_json::Value) -> DynamicObject {
    serde_json::from_value(serde_json::json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "shop" },
        "spec": spec,
    }))
    .unwrap()
}

#[test]
fn a_workloads_selector_is_its_match_labels_and_expressions() {
    let deployment = object(serde_json::json!({
        "selector": {
            "matchLabels": { "app": "web" },
            "matchExpressions": [{ "key": "tier", "operator": "In", "values": ["front"] }],
        },
    }));
    assert_eq!(
        of_object(&deployment).unwrap(),
        parse("app=web,tier in (front)").unwrap()
    );
}

#[test]
fn a_services_selector_is_its_label_map() {
    let service = object(serde_json::json!({ "selector": { "app": "web" } }));
    assert_eq!(of_object(&service).unwrap(), parse("app=web").unwrap());
}

#[test]
fn no_selector_or_an_empty_one_selects_no_pods_to_stream() {
    assert!(of_object(&object(serde_json::json!({ "replicas": 1 }))).is_none());
    assert!(of_object(&object(serde_json::json!({ "selector": {} }))).is_none());
}
