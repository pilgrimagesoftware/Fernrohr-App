//! #244: each requirement form reads as words, from the selector or its text.

use super::{readable, readable_text};
use crate::k8s::label_selector::parse;

fn read(text: &str) -> String {
    readable(&parse(text).expect("the selector parses"))
}

#[test]
fn a_single_value_set_reads_as_the_value() {
    assert_eq!(read("tier in (front)"), "tier: front");
}

#[test]
fn a_set_of_several_lists_them() {
    assert_eq!(read("tier in (front,back)"), "tier: back, front");
}

#[test]
fn equality_reads_as_the_value() {
    assert_eq!(read("app=web"), "app: web");
    assert_eq!(read("app==web"), "app: web");
}

#[test]
fn negations_read_as_not() {
    assert_eq!(read("env!=dev"), "env: not dev");
    assert_eq!(read("env notin (dev,test)"), "env: not dev, test");
}

#[test]
fn existence_reads_as_has_or_no() {
    assert_eq!(read("canary"), "has canary");
    assert_eq!(read("!canary"), "no canary");
}

#[test]
fn an_empty_value_is_named() {
    assert_eq!(read("tier="), "tier: (empty)");
}

#[test]
fn requirements_are_kept_in_order_and_set_apart() {
    assert_eq!(
        read("app=web,tier in (front),!canary"),
        "app: web \u{b7} tier: front \u{b7} no canary"
    );
}

#[test]
fn text_that_does_not_parse_is_shown_as_it_is() {
    assert_eq!(readable_text("tier in (front"), "tier in (front");
    assert_eq!(readable_text("app=web"), "app: web");
}
