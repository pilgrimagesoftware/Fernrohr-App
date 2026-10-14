//! A label or annotation key, checked in the prompt as the API server checks
//! it - a qualified name: an optional DNS-subdomain prefix and `/`, then a name
//! of at most 63 characters, alphanumeric at both ends with `-`, `_` and `.`
//! between - so a bad key is refused once, not once per checked object.

/// The longest name part (and prefix label), and the longest prefix.
const NAME_MAX: usize = 63;
const PREFIX_MAX: usize = 253;

/// Whether `key` is a valid label or annotation key, or why not.
pub(super) fn check(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Enter a key.".to_string());
    }
    let (prefix, name) = match key.split_once('/') {
        Some((prefix, name)) => (Some(prefix), name),
        None => (None, key),
    };
    if let Some(prefix) = prefix {
        check_prefix(prefix)?;
    }
    check_name(name)
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("A key needs a name after its prefix's \"/\".".to_string());
    }
    if name.len() > NAME_MAX {
        return Err(format!(
            "A key's name can be at most {NAME_MAX} characters."
        ));
    }
    let alphanumeric = |c: char| c.is_ascii_alphanumeric();
    let ends_ok = name.starts_with(alphanumeric) && name.ends_with(alphanumeric);
    let chars_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ends_ok && chars_ok {
        Ok(())
    } else {
        Err(
            "A key's name uses letters, digits, \"-\", \"_\" and \".\", and starts and ends \
             with a letter or digit."
                .to_string(),
        )
    }
}

/// A DNS subdomain: lowercase labels of letters, digits and `-`, joined by `.`.
fn check_prefix(prefix: &str) -> Result<(), String> {
    let label_ok = |label: &str| {
        !label.is_empty()
            && label.len() <= NAME_MAX
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    if !prefix.is_empty() && prefix.len() <= PREFIX_MAX && prefix.split('.').all(label_ok) {
        Ok(())
    } else {
        Err(
            "A key's prefix is a DNS subdomain, such as example.com: lowercase letters, \
             digits, \"-\" and \".\"."
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn qualified_names_are_taken() {
        for key in [
            "team",
            "app.kubernetes.io/name",
            "a",
            "Team_1.x",
            "example.com/x-y",
        ] {
            assert_eq!(check(key), Ok(()), "{key}");
        }
    }

    #[test]
    fn anything_the_api_server_refuses_is_refused() {
        let long = "a".repeat(64);
        for key in [
            "",
            "a b",
            "team=platform",
            "-team",
            "team-",
            "Example.com/x",
            "example.com/",
            "/x",
            "ex_ample.com/x",
            "a/b/c",
            long.as_str(),
        ] {
            assert!(check(key).is_err(), "{key:?}");
        }
    }
}
