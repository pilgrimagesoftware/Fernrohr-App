//! The prompts' input rules, and Open Host's URL.

use super::host_url;
use crate::k8s::resource::kind_actions::{Taint, TaintEffect};
use crate::ui::kind_actions::edits::parse_size;
use crate::ui::kind_actions::taints::parse;

#[test]
fn expand_takes_only_a_larger_size() {
    assert_eq!(parse_size("20Gi", "10Gi"), Ok("20Gi".into()));
    assert!(parse_size("10Gi", "10Gi").is_err(), "the same size");
    assert!(parse_size("5Gi", "10Gi").is_err(), "smaller");
    assert!(parse_size("10G", "10Gi").is_err(), "10G is under 10Gi");
    assert!(parse_size("twenty", "10Gi").is_err());
}

#[test]
fn a_taint_needs_a_valid_key_and_effect_and_mustnt_exist_yet() {
    let taint = parse("dedicated", "gpu", "NoSchedule", &[]).expect("a taint");
    assert_eq!(taint.label(), "dedicated=gpu:NoSchedule");
    assert_eq!(
        parse("dedicated", "", "noexecute", &[]).unwrap().label(),
        "dedicated:NoExecute"
    );
    assert!(parse("Bad Key", "", "NoSchedule", &[]).is_err());
    assert!(parse("dedicated", "", "Never", &[]).is_err());
    let existing = [Taint::new(
        "dedicated",
        Some("cpu"),
        TaintEffect::NoSchedule,
    )];
    assert!(
        parse("dedicated", "gpu", "NoSchedule", &existing).is_err(),
        "the same key and effect"
    );
    assert!(parse("dedicated", "gpu", "NoExecute", &existing).is_ok());
}

#[test]
fn open_host_opens_the_host_over_https() {
    assert_eq!(host_url("app.example.com"), "https://app.example.com");
}
