//! `command-tunnels` 1.2: splitting, line continuations, and `{port}`.

use super::*;

fn args(line: &str) -> Vec<String> {
    split(line).expect("splits")
}

#[test]
fn quoted_arguments_stay_whole_without_their_quotes() {
    assert_eq!(
        args(r#"tool --name "two words" --other 'single quoted' plain"#),
        [
            "tool",
            "--name",
            "two words",
            "--other",
            "single quoted",
            "plain"
        ]
    );
}

#[test]
fn a_pasted_multi_line_command_is_one_command() {
    let pasted = "gcloud compute ssh <host> \\\n    --tunnel-through-iap \\\r\n    --project \"<project>\" -- -N -L{port}:127.0.0.1:8888";
    assert_eq!(
        args(pasted),
        [
            "gcloud",
            "compute",
            "ssh",
            "<host>",
            "--tunnel-through-iap",
            "--project",
            "<project>",
            "--",
            "-N",
            "-L{port}:127.0.0.1:8888",
        ]
    );
}

#[test]
fn unbalanced_quotes_and_empty_lines_are_errors() {
    assert_eq!(split("ssh 'unclosed"), Err(ArgvError::UnbalancedQuotes));
    assert_eq!(split("ssh \"unclosed"), Err(ArgvError::UnbalancedQuotes));
    assert_eq!(split("   \n  "), Err(ArgvError::Empty));
}

#[test]
fn every_port_placeholder_is_replaced() {
    let line = args("ssh -L{port}:127.0.0.1:8888 -o Port={port} host");
    assert!(has_port_placeholder(&line));
    assert_eq!(
        substitute_port(&line, 53124),
        ["ssh", "-L53124:127.0.0.1:8888", "-o", "Port=53124", "host"]
    );
    assert!(!has_port_placeholder(&args(
        "ssh -L8888:127.0.0.1:8888 host"
    )));
}

/// Substitution happens after splitting, so the argument count can't change -
/// whatever the port, `{port}` inside one argument stays inside that argument.
#[test]
fn a_port_cannot_change_how_the_line_splits() {
    let line = args("tool '{port} quoted' {port}");
    let substituted = substitute_port(&line, 1);
    assert_eq!(substituted, ["tool", "1 quoted", "1"]);
    assert_eq!(substituted.len(), line.len());
}
