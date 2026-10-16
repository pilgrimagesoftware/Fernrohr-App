//! The exec library's rules: every validation error, the tidying a saved entry
//! gets, duplicates, and the TOML shape - which keeps entry order.

use super::{Entry, EntryError, ExecLibrary, Field, Target, WorkloadKind, split_command};

fn entry(name: &str, command: &str, target: Target) -> Entry {
    Entry {
        name: name.into(),
        command: command.into(),
        container: None,
        target,
    }
}

fn selector(namespace: Option<&str>, selector: &str) -> Target {
    Target::Selector {
        namespace: namespace.map(str::to_string),
        selector: selector.into(),
    }
}

fn workload(namespace: &str, name: &str) -> Target {
    Target::Workload {
        namespace: namespace.into(),
        workload_kind: WorkloadKind::Deployment,
        name: name.into(),
    }
}

fn library(names: &[&str]) -> ExecLibrary {
    let mut library = ExecLibrary::default();
    for name in names {
        library
            .create(entry(name, "date", Target::Any))
            .expect("valid");
    }
    library
}

#[test]
fn a_name_is_unique_ignoring_case_and_surrounding_spaces() {
    let mut library = library(&["Stream info"]);
    let error = library
        .create(entry(" stream INFO ", "date", Target::Any))
        .unwrap_err();
    assert_eq!(error, EntryError::NameTaken);
    assert_eq!(error.field(), Some(Field::Name));
    assert_eq!(library.entries.len(), 1, "nothing saved");
}

#[test]
fn every_field_error_names_its_field() {
    let library = ExecLibrary::default();
    let cases = [
        (
            entry("  ", "date", Target::Any),
            EntryError::EmptyName,
            Field::Name,
        ),
        (
            entry("x", "   ", Target::Any),
            EntryError::EmptyCommand,
            Field::Command,
        ),
        (
            entry("x", "echo \"unterminated", Target::Any),
            EntryError::UnbalancedQuotes,
            Field::Command,
        ),
        (
            entry("x", "date", workload("  ", "web")),
            EntryError::NoNamespace,
            Field::Namespace,
        ),
        (
            entry("x", "date", workload("shop", " ")),
            EntryError::NoWorkloadName,
            Field::WorkloadName,
        ),
    ];
    for (entry, expected, field) in cases {
        let error = library.check(entry, None).unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(error.field(), Some(field), "{expected:?}");
    }
    let bad_selector = library
        .check(entry("x", "date", selector(None, "app in (web")), None)
        .unwrap_err();
    assert!(
        matches!(bad_selector, EntryError::Selector(_)),
        "{bad_selector:?}"
    );
    assert_eq!(bad_selector.field(), Some(Field::Selector));
}

#[test]
fn a_saved_entry_is_trimmed_and_drops_empty_optionals() {
    let mut library = ExecLibrary::default();
    let mut raw = entry(
        "  Stream info ",
        " nats stream info X ",
        selector(Some("  "), " app=nats "),
    );
    raw.container = Some("  ".into());
    library.create(raw).expect("valid");
    assert_eq!(
        library.entries[0],
        entry(
            "Stream info",
            "nats stream info X",
            selector(None, "app=nats")
        )
    );
}

#[test]
fn a_command_splits_with_posix_quoting() {
    assert_eq!(
        split_command(r#"sh -c 'echo "$HOME" | wc -c'"#).unwrap(),
        ["sh", "-c", r#"echo "$HOME" | wc -c"#]
    );
    assert_eq!(split_command(""), Err(EntryError::EmptyCommand));
}

#[test]
fn an_edit_keeps_its_place_and_may_keep_its_own_name() {
    let mut library = library(&["a", "b", "c"]);
    library
        .update("b", entry("B", "uptime", Target::Any))
        .expect("its own name, recased, isn't taken");
    let names: Vec<&str> = library.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a", "B", "c"]);
    assert_eq!(
        library.update("a", entry("c", "date", Target::Any)),
        Err(EntryError::NameTaken)
    );
    assert_eq!(
        library.update("zz", entry("zz", "date", Target::Any)),
        Err(EntryError::NotFound)
    );
}

#[test]
fn a_duplicate_lands_after_its_original_with_an_untaken_name() {
    let mut library = library(&["a", "a copy", "b"]);
    assert_eq!(library.duplicate("a").unwrap(), "a copy 2");
    let names: Vec<&str> = library.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a", "a copy 2", "a copy", "b"]);
    assert!(library.remove("A COPY 2"));
    assert!(!library.remove("missing"));
}

#[test]
fn the_toml_shape_is_the_designs_and_keeps_order() {
    let text = r#"
[[entries]]
name = "NATS stream info"
command = "nats stream info X"
container = "nats-box"

[entries.target]
kind = "selector"
namespace = "nats-system"
selector = "app.kubernetes.io/component=nats-box"

[[entries]]
name = "Web shell"
command = "sh"

[entries.target]
kind = "workload"
namespace = "shop"
workload_kind = "Deployment"
name = "web"

[[entries]]
name = "Uptime"
command = "uptime"

[entries.target]
kind = "any"
"#;
    let library: ExecLibrary = toml::from_str(text).expect("parses");
    assert_eq!(library.entries.len(), 3);
    assert_eq!(library.entries[0].container.as_deref(), Some("nats-box"));
    assert_eq!(
        library.entries[0].target,
        selector(Some("nats-system"), "app.kubernetes.io/component=nats-box")
    );
    assert_eq!(library.entries[1].target, workload("shop", "web"));
    assert_eq!(library.entries[2].target, Target::Any);

    let written = toml::to_string_pretty(&library).expect("serializes");
    let back: ExecLibrary = toml::from_str(&written).expect("parses again");
    assert_eq!(back, library, "a round trip keeps every entry, in order");
}
