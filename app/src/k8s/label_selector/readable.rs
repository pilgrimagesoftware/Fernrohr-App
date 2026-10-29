//! A selector as a person reads it (#244), for wherever one is shown rather
//! than typed: `tier in (front)` reads `tier: front`. The `kubectl -l` text
//! stays the selector's own form - what the user types, and what a tooltip
//! beside the readable text shows.

use kube::core::{Expression, Selector};

/// Between requirements: a selector picks pods matching every one of them.
const BETWEEN: &str = " \u{b7} ";

/// `selector`'s requirements, each as `key: value` - several values listed,
/// `not` before what the key mustn't be, `has key` or `no key` for one only
/// required present or absent - in the selector's order.
pub fn readable(selector: &Selector) -> String {
    selector
        .clone()
        .into_iter()
        .map(|expression| requirement(&expression))
        .collect::<Vec<_>>()
        .join(BETWEEN)
}

/// [`readable`] for a selector's `kubectl -l` text, or the text unchanged when
/// it doesn't parse.
pub fn readable_text(text: &str) -> String {
    super::parse(text).map_or_else(|_| text.to_string(), |selector| readable(&selector))
}

fn requirement(expression: &Expression) -> String {
    match expression {
        Expression::Equal(key, value) => format!("{key}: {}", value_text(value)),
        Expression::In(key, values) => format!("{key}: {}", values_text(values)),
        Expression::NotEqual(key, value) => format!("{key}: not {}", value_text(value)),
        Expression::NotIn(key, values) => format!("{key}: not {}", values_text(values)),
        Expression::Exists(key) => format!("has {key}"),
        Expression::DoesNotExist(key) => format!("no {key}"),
    }
}

/// A value, with an empty one - a label set to nothing - named as such.
fn value_text(value: &str) -> &str {
    if value.is_empty() { "(empty)" } else { value }
}

fn values_text<'a>(values: impl IntoIterator<Item = &'a String>) -> String {
    values
        .into_iter()
        .map(|value| value_text(value))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
