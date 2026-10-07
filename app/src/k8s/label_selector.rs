//! Label selectors (#150): parsed from the text `kubectl -l` takes, read off a
//! workload's `spec.selector`, and matched against a pod's labels. The
//! selector itself is `kube`'s [`Selector`]; this module is the parsing and
//! extraction it lacks. What a selector is used *for* - streaming logs - is
//! `util::logs`'.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::DynamicObject;
use kube::core::{Expression, Selector, SelectorExt as _};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Why a selector's text didn't parse: the requirement at fault, as typed, and
/// what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorError {
    pub requirement: String,
    pub reason: &'static str,
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.requirement.is_empty() {
            write!(f, "{}", self.reason)
        } else {
            write!(f, "\u{201c}{}\u{201d}: {}", self.requirement, self.reason)
        }
    }
}

impl std::error::Error for SelectorError {}

/// Parses `text` as `kubectl -l` does: comma-separated requirements, each
/// `key`, `!key`, `key=value`, `key==value`, `key!=value`, `key in (a,b)` or
/// `key notin (a,b)`. Blank text is the empty selector, which selects every
/// pod - the caller decides whether that is allowed.
pub fn parse(text: &str) -> Result<Selector, SelectorError> {
    if text.trim().is_empty() {
        return Ok(Selector::default());
    }
    split_requirements(text)?
        .into_iter()
        .map(requirement)
        .collect()
}

/// `text` cut at the commas between requirements - not those inside an
/// `in (..)` set.
fn split_requirements(text: &str) -> Result<Vec<&str>, SelectorError> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (ix, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1).ok_or_else(|| SelectorError {
                    requirement: text.trim().to_string(),
                    reason: "has a \u{201c})\u{201d} with no \u{201c}(\u{201d}",
                })?;
            }
            ',' if depth == 0 => {
                parts.push(&text[start..ix]);
                start = ix + 1;
            }
            _ => {}
        }
    }
    if depth > 0 {
        return Err(SelectorError {
            requirement: text[start..].trim().to_string(),
            reason: "has a \u{201c}(\u{201d} that isn't closed",
        });
    }
    parts.push(&text[start..]);
    Ok(parts)
}

/// One requirement's text as an [`Expression`].
fn requirement(text: &str) -> Result<Expression, SelectorError> {
    let text = text.trim();
    let error = |reason| SelectorError {
        requirement: text.to_string(),
        reason,
    };
    if text.is_empty() {
        return Err(error("is empty - remove the extra comma"));
    }
    if let Some(key) = text.strip_prefix('!') {
        return Ok(Expression::DoesNotExist(key_of(key.trim()).map_err(error)?));
    }
    if text.contains('(') {
        let (key, rest) = text.split_once(char::is_whitespace).ok_or_else(|| {
            error("needs \u{201c}in\u{201d} or \u{201c}notin\u{201d} before its set")
        })?;
        let rest = rest.trim_start();
        let (negated, set) = if let Some(set) = rest.strip_prefix("notin") {
            (true, set)
        } else if let Some(set) = rest.strip_prefix("in") {
            (false, set)
        } else {
            return Err(error(
                "needs \u{201c}in\u{201d} or \u{201c}notin\u{201d} before its set",
            ));
        };
        let set = set
            .trim()
            .strip_prefix('(')
            .and_then(|set| set.strip_suffix(')'))
            .ok_or_else(|| error("needs its values in parentheses"))?;
        let values = set
            .split(',')
            .map(|value| value_of(value.trim()))
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(error)?;
        if values.is_empty() || values.contains("") {
            return Err(error("has an empty value in its set"));
        }
        let key = key_of(key).map_err(error)?;
        return Ok(if negated {
            Expression::NotIn(key, values)
        } else {
            Expression::In(key, values)
        });
    }
    if let Some((key, value)) = text.split_once("!=") {
        let key = key_of(key.trim()).map_err(error)?;
        return Ok(Expression::NotEqual(
            key,
            value_of(value.trim()).map_err(error)?,
        ));
    }
    if let Some((key, value)) = text.split_once('=') {
        // `==` is `=`.
        let value = value.strip_prefix('=').unwrap_or(value);
        let key = key_of(key.trim()).map_err(error)?;
        return Ok(Expression::Equal(
            key,
            value_of(value.trim()).map_err(error)?,
        ));
    }
    Ok(Expression::Exists(key_of(text).map_err(error)?))
}

/// A label key: an optional `prefix/` then a name, of the characters
/// Kubernetes allows. The server enforces the finer rules; this catches typos.
fn key_of(key: &str) -> Result<String, &'static str> {
    let name = key.rsplit_once('/').map_or(key, |(_, name)| name);
    if name.is_empty() {
        return Err("needs a label key");
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/'))
    {
        return Err("has a character a label key can't contain");
    }
    Ok(key.to_string())
}

/// A label value: possibly empty, of the characters Kubernetes allows.
fn value_of(value: &str) -> Result<String, &'static str> {
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err("has a character a label value can't contain");
    }
    Ok(value.to_string())
}

/// The pods `object` manages, as its `spec.selector` names them: a
/// `LabelSelector` for a workload (Deployment, StatefulSet, DaemonSet,
/// ReplicaSet, Job), or a plain label map for a Service. `None` when the
/// object has no selector, or one that selects every pod - no workload's.
pub fn of_object(object: &DynamicObject) -> Option<Selector> {
    let selector = object.data.get("spec")?.get("selector")?;
    let map = selector.as_object()?;
    let selector = if map.contains_key("matchLabels") || map.contains_key("matchExpressions") {
        let selector: LabelSelector = serde_json::from_value(selector.clone()).ok()?;
        Selector::try_from(selector).ok()?
    } else {
        map.iter()
            .map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .collect()
    };
    (!selector.selects_all()).then_some(selector)
}

/// Whether a pod with `labels` (none at all reads as empty) is one `selector`
/// selects.
pub fn matches(selector: &Selector, labels: Option<&BTreeMap<String, String>>) -> bool {
    match labels {
        Some(labels) => selector.matches(labels),
        None => selector.matches(&BTreeMap::new()),
    }
}

#[cfg(test)]
mod tests;
