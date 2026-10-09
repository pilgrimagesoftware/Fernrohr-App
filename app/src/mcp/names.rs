//! Names a client passes that end up in a request path: object names,
//! namespaces and container names, parsed into types that can only hold a
//! safe value.
//!
//! kube builds an object's URL by joining its name onto the collection path
//! unescaped, so a name is part of the path, not data inside it. A name like
//! `../secrets/db` would address a different collection than the kind the tool
//! resolved - one its Secret redaction never sees - and `x?watch=true` would
//! smuggle in a query. These types refuse anything that isn't a single path
//! segment the API server itself would accept, before a request is built.

use super::error::ToolError;

/// The longest name the API server accepts for any object (a DNS subdomain).
const MAX_OBJECT_NAME: usize = 253;
/// The longest namespace or container name (a DNS label).
const MAX_LABEL: usize = 63;

/// An object's name: one path segment of printable ASCII, without `/`, `%`,
/// `?` or `#`, and not `.` or `..` - the API server's own path-segment rule,
/// which RBAC names like `system:node-proxier` pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ObjectName(String);

impl ObjectName {
    /// Parses the argument `field`'s `value`.
    pub(super) fn parse(field: &str, value: &str) -> Result<Self, ToolError> {
        let segment_safe = value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'/' | b'%' | b'?' | b'#'));
        if value.is_empty()
            || value.len() > MAX_OBJECT_NAME
            || !segment_safe
            || value == "."
            || value == ".."
        {
            return Err(invalid(field, value, "is not a valid object name"));
        }
        Ok(Self(value.to_string()))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A namespace, or a container's name: a DNS-1123 label - lowercase letters,
/// digits and `-`, starting and ending with a letter or digit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LabelName(String);

impl LabelName {
    /// Parses the argument `field`'s `value`.
    pub(super) fn parse(field: &str, value: &str) -> Result<Self, ToolError> {
        let bytes = value.as_bytes();
        let allowed = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_LABEL
            && bytes.iter().all(|byte| allowed(byte) || *byte == b'-')
            && bytes.first().is_some_and(allowed)
            && bytes.last().is_some_and(allowed);
        if !valid {
            return Err(invalid(field, value, "is not a valid DNS label"));
        }
        Ok(Self(value.to_string()))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

fn invalid(field: &str, value: &str, problem: &str) -> ToolError {
    ToolError::InvalidArguments {
        message: format!("`{field}` {value:?} {problem}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_object_names_parse() {
        for name in [
            "api-7d9f",
            "web.example.com",
            "system:node-proxier",
            "a",
            &"x".repeat(253),
        ] {
            assert_eq!(ObjectName::parse("name", name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn names_that_would_change_the_request_path_are_refused() {
        for name in [
            "",
            ".",
            "..",
            "../secrets/db",
            "pod/log",
            "x?watch=true",
            "x#y",
            "%2e%2e",
            "with space",
            "tab\t",
            "é",
            &"x".repeat(254),
        ] {
            assert!(
                matches!(
                    ObjectName::parse("name", name),
                    Err(ToolError::InvalidArguments { .. })
                ),
                "{name:?}"
            );
        }
    }

    #[test]
    fn labels_are_dns_labels() {
        for label in ["default", "team-a", "a1", &"n".repeat(63)] {
            assert_eq!(
                LabelName::parse("namespace", label).unwrap().as_str(),
                label
            );
        }
        for label in ["", "Team", "-a", "a-", "a.b", "a/b", "..", &"n".repeat(64)] {
            assert!(LabelName::parse("namespace", label).is_err(), "{label:?}");
        }
    }

    #[test]
    fn the_error_names_the_argument() {
        let Err(ToolError::InvalidArguments { message }) = LabelName::parse("namespace", "A")
        else {
            panic!("an uppercase namespace parses");
        };
        assert!(message.starts_with("`namespace`"), "{message}");
    }
}
