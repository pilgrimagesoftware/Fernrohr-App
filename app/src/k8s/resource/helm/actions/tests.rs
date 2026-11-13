//! helm-releases section 8: Rollback and Uninstall. Unit tests for what the
//! confirmations say and how a failure reads; window tests, over a fake
//! cluster and a fake `helm` script, for every route by keystrokes alone.

use super::RevisionLine;
use super::flow::refusal_of;
use super::manifest::count_objects;
use crate::helm_cli::HelmFailure;

mod detail;
mod fixture;
mod list;

/// A manifest's objects are its documents with a `kind`; Helm's `# Source`
/// comments, blank documents and broken ones aren't counted.
#[test]
fn a_manifest_counts_its_objects() {
    let manifest = "---\n# Source: web/templates/service.yaml\napiVersion: v1\nkind: Service\n\
                    metadata:\n  name: web\n---\n# Source: web/templates/deployment.yaml\n\
                    apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n---\n\
                    # Source: web/templates/empty.yaml\n---\n";
    assert_eq!(count_objects(manifest), Some(2));
    assert_eq!(count_objects(""), Some(0));
}

/// A document that doesn't parse ends the count with no number - the YAML
/// reader would otherwise yield errors forever - so the confirmation says
/// "the objects" rather than a wrong count.
#[test]
fn a_broken_manifest_has_no_count() {
    let manifest = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\n---\n: not yaml: [\n\
                    ---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: web\n";
    assert_eq!(count_objects(manifest), None);
}

#[test]
fn a_revision_reads_with_its_chart_and_app() {
    let line = RevisionLine {
        version: 3,
        chart: Some("web-1.3.0".into()),
        app_version: Some("2.0".into()),
    };
    assert_eq!(
        line.describe(),
        "revision 3 \u{00b7} web-1.3.0 \u{00b7} app 2.0"
    );
    let unread = RevisionLine {
        version: 2,
        chart: None,
        app_version: None,
    };
    assert_eq!(unread.describe(), "revision 2");
}

/// A failed run reads as `helm`'s status, with its own error as the detail.
#[test]
fn a_failure_shows_helms_message() {
    let refusal = refusal_of(
        "Uninstall web",
        HelmFailure::Exit {
            code: Some(1),
            message: "Error: pre-delete hook failed".into(),
        },
    );
    assert_eq!(refusal.action, "Uninstall web");
    assert_eq!(refusal.failure.message, "helm exited with status 1");
    assert_eq!(refusal.failure.detail, "Error: pre-delete hook failed");
}

/// The new keys clash with nothing in their panels' scopes.
#[test]
fn the_action_keys_clash_with_nothing() {
    use crate::keymap::KeymapConfig;
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    for id in [
        "helm.uninstall_release",
        "helm_release.rollback",
        "helm_release.uninstall",
    ] {
        let command = registry.get(id).expect("registered");
        let clashes = crate::keymap::conflicts(
            &registry,
            &KeymapConfig::default(),
            id,
            command.default_binding,
        );
        assert!(!clashes.any_clash(), "{id} clashes: {clashes:?}");
    }
}
