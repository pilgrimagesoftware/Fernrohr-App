//! A release's rendered manifest made safe to show (helm-releases D6): split
//! into its YAML documents, every Secret's `data`, `stringData` and
//! last-applied annotation replaced by their sizes - also inside a `List` -
//! and any document that doesn't parse withheld rather than shown raw, since
//! it can't be checked. Every other document is kept exactly as written.

use crate::k8s::resource::object_detail::redact_secret_json;
use serde_json::Value;

/// What stands in for a document that couldn't be read.
pub(crate) const WITHHELD: &str = "# A document that couldn't be read is withheld.";

/// `manifest` with its Secrets redacted and unreadable documents withheld.
pub(crate) fn redacted_manifest(manifest: &str) -> String {
    documents(manifest)
        .into_iter()
        .filter_map(redacted_document)
        .map(|document| format!("---\n{}\n", document.trim_end()))
        .collect()
}

/// The documents of a multi-document YAML stream: split at each line that
/// starts a document (`---`, possibly followed by a comment). Each is parsed
/// on its own, so a broken one costs only itself (also the Resources tab's).
pub(crate) fn documents(manifest: &str) -> Vec<String> {
    let mut documents = vec![String::new()];
    for line in manifest.lines() {
        if line == "---" || line.starts_with("--- ") {
            documents.push(String::new());
        } else if let Some(current) = documents.last_mut() {
            current.push_str(line);
            current.push('\n');
        }
    }
    documents
        .into_iter()
        .filter(|document| !document.trim().is_empty())
        .collect()
}

/// One document as it may be shown, or `None` for one with no content.
fn redacted_document(document: String) -> Option<String> {
    let Ok(mut value) = serde_yaml_ng::from_str::<Value>(&document) else {
        return Some(WITHHELD.to_string());
    };
    if value.is_null() {
        // Only comments, such as Helm's `# Source:` line of an empty template.
        return None;
    }
    if !redact_secrets(&mut value) {
        return Some(document);
    }
    // Only Helm's own `# Source:` line: a template's other comments are
    // rendered too, and can carry a value (`# password: {{ .Values.x }}`).
    let comments: String = document
        .lines()
        .take_while(|line| line.starts_with('#'))
        .filter(|line| line.starts_with("# Source: "))
        .map(|line| format!("{line}\n"))
        .collect();
    match serde_yaml_ng::to_string(&value) {
        Ok(yaml) => Some(format!("{comments}{yaml}")),
        Err(_) => Some(WITHHELD.to_string()),
    }
}

/// Redacts every Secret in `value` - itself, or the items of a `List` -
/// and says whether it found one.
fn redact_secrets(value: &mut Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("Secret") => {
            redact_secret_json(value);
            true
        }
        Some(kind) if kind.ends_with("List") => {
            let mut found = false;
            if let Some(Value::Array(items)) = value.get_mut("items") {
                for item in items {
                    found |= redact_secrets(item);
                }
            }
            found
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{WITHHELD, redacted_manifest};

    const MANIFEST: &str = "---
# Source: web/templates/secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: web-credentials
data:
  password: aHVudGVyMg==
stringData:
  token: s3cr3t-token
---
# Source: web/templates/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: web-config
data:
  mode: production
";

    #[test]
    fn a_secrets_values_become_their_sizes() {
        let shown = redacted_manifest(MANIFEST);
        assert!(!shown.contains("aHVudGVyMg=="), "{shown}");
        assert!(!shown.contains("s3cr3t-token"), "{shown}");
        assert!(
            shown.contains("password: '<redacted: 7 bytes>'")
                || shown.contains("password: <redacted: 7 bytes>"),
            "{shown}"
        );
        assert!(shown.contains("token:"), "keys stay: {shown}");
        assert!(
            shown.contains("# Source: web/templates/secret.yaml"),
            "comments stay"
        );
    }

    #[test]
    fn other_documents_stay_as_written() {
        let shown = redacted_manifest(MANIFEST);
        assert!(
            shown.contains(
                "# Source: web/templates/configmap.yaml\napiVersion: v1\nkind: ConfigMap"
            ),
            "{shown}"
        );
        assert!(shown.contains("mode: production"));
        assert_eq!(shown.matches("---\n").count(), 2, "two documents");
    }

    /// A Secret's own comments are template output and can carry a value:
    /// only Helm's `# Source:` line is kept.
    #[test]
    fn a_secrets_comments_other_than_source_are_dropped() {
        let manifest = "---\n# Source: web/templates/secret.yaml\n# password: hunter2\napiVersion: v1\nkind: Secret\nmetadata:\n  name: a\nstringData:\n  password: hunter2\n";
        let shown = redacted_manifest(manifest);
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(
            shown.contains("# Source: web/templates/secret.yaml"),
            "{shown}"
        );
    }

    #[test]
    fn a_secret_inside_a_list_is_redacted() {
        let manifest = "---\napiVersion: v1\nkind: List\nitems:\n- apiVersion: v1\n  kind: Secret\n  metadata:\n    name: a\n  stringData:\n    key: hidden-value\n";
        let shown = redacted_manifest(manifest);
        assert!(!shown.contains("hidden-value"), "{shown}");
    }

    #[test]
    fn a_document_that_doesnt_parse_is_withheld_not_shown() {
        let manifest = "---\nkind: Secret\ndata:\n  password: [unclosed\n---\nkind: ConfigMap\n";
        let shown = redacted_manifest(manifest);
        assert!(!shown.contains("unclosed"), "{shown}");
        assert!(shown.contains(WITHHELD));
        assert!(shown.contains("kind: ConfigMap"));
    }

    #[test]
    fn the_last_applied_annotation_of_a_secret_is_redacted() {
        let manifest = "---\napiVersion: v1\nkind: Secret\nmetadata:\n  name: a\n  annotations:\n    kubectl.kubernetes.io/last-applied-configuration: '{\"data\":{\"k\":\"c2VjcmV0\"}}'\n";
        let shown = redacted_manifest(manifest);
        assert!(!shown.contains("c2VjcmV0"), "{shown}");
    }
}
