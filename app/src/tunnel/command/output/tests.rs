use super::*;

#[test]
fn keeps_only_the_most_recent_lines() {
    let tail = OutputTail::default();
    for n in 0..COMMAND_TUNNEL_OUTPUT_LINES + 5 {
        tail.push(format!("line {n}"));
    }
    let text = tail.text();
    assert!(!text.contains("line 4\n"), "the oldest lines dropped");
    assert!(text.starts_with("line 5\n"));
    assert!(text.ends_with(&format!("line {}", COMMAND_TUNNEL_OUTPUT_LINES + 4)));
}

#[test]
fn a_reason_carries_the_output_only_when_there_is_some() {
    let tail = OutputTail::default();
    assert_eq!(tail.with_reason("exited"), "exited");
    tail.push("ERROR: please log in".into());
    assert_eq!(
        tail.with_reason("exited"),
        "exited\n\nRecent output:\nERROR: please log in"
    );
}

/// #423: a prompt doesn't end its line, and it's still in the output.
#[test]
fn output_without_a_newline_is_kept() {
    let tail = OutputTail::default();
    tail.take(
        Pipe::Stdout,
        b"Testing tunnel.\r\nEnter passphrase for key 'k': ",
    );
    assert_eq!(
        tail.text(),
        "Testing tunnel.\nEnter passphrase for key 'k': "
    );
}

/// Reads split anywhere - mid-line, mid-character - give the same lines.
#[test]
fn lines_and_characters_split_across_reads_are_joined() {
    let tail = OutputTail::default();
    let bytes = "first\nzwe\u{ff}i\nthird".as_bytes();
    let (a, b) = bytes.split_at(10);
    tail.take(Pipe::Stdout, a);
    tail.take(Pipe::Stdout, b);
    assert_eq!(tail.text(), "first\nzwe\u{ff}i\nthird");
}

/// stdout and stderr writing parts of lines at once don't splice together.
#[test]
fn each_pipe_keeps_its_own_unfinished_line() {
    let tail = OutputTail::default();
    tail.take(Pipe::Stdout, b"out ");
    tail.take(Pipe::Stderr, b"err ");
    tail.take(Pipe::Stdout, b"done\n");
    assert_eq!(tail.text(), "out done\nerr ");
}

#[test]
fn an_endless_line_is_cut_at_the_limit() {
    let tail = OutputTail::default();
    tail.take(Pipe::Stdout, &vec![b'.'; PARTIAL_LIMIT + 10]);
    let text = tail.text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].len(), PARTIAL_LIMIT);
}

/// #423: a prompt counts once the command has been quiet at it a while.
#[test]
fn a_prompt_counts_only_once_the_command_is_quiet() {
    let tail = OutputTail::default();
    assert_eq!(tail.waiting_prompt(Duration::ZERO), None, "nothing printed");
    tail.take(Pipe::Stderr, b"Enter passphrase for key 'k': ");
    assert_eq!(
        tail.waiting_prompt(Duration::from_secs(60)),
        None,
        "not quiet yet"
    );
    assert_eq!(
        tail.waiting_prompt(Duration::ZERO).as_deref(),
        Some("Enter passphrase for key 'k':")
    );
}

/// A prompt the command printed with a newline, as some do, is still one -
/// but only while it's the last thing printed.
#[test]
fn a_finished_last_line_can_be_the_prompt() {
    let tail = OutputTail::default();
    tail.take(Pipe::Stdout, b"Please enter your password:\n");
    assert!(tail.waiting_prompt(Duration::ZERO).is_some());
    tail.take(Pipe::Stdout, b"Authenticated.\n");
    assert_eq!(tail.waiting_prompt(Duration::ZERO), None);
}

#[test]
fn ordinary_output_is_no_prompt() {
    let tail = OutputTail::default();
    tail.take(Pipe::Stdout, b"Listening on port [4000].\n");
    assert_eq!(tail.waiting_prompt(Duration::ZERO), None);
}
