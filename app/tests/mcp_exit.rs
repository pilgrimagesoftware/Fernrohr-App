//! `agent-mcp` "MCP command exits with its client" on the built binary
//! (cli-help-and-shutdown task 2.2). No app runs: the adapter only reaches for
//! the app on a tool request, and none is sent.
//!
//! Each test first completes the MCP handshake, which proves the adapter is
//! serving (with its signal and parent watches installed) before the client
//! goes away.

#![cfg(unix)]

mod support;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use support::FreshHome;

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"0"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

/// Sends the handshake on `stdin` and waits for the server's reply on `stdout`.
fn handshake(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    writeln!(stdin, "{INITIALIZE}").expect("send initialize");
    let mut reply = String::new();
    stdout.read_line(&mut reply).expect("read initialize reply");
    assert!(reply.contains("\"serverInfo\""), "{reply}");
    writeln!(stdin, "{INITIALIZED}").expect("send initialized");
    stdin.flush().expect("flush");
}

fn spawn_mcp(home: &FreshHome) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = home
        .command(&["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fernrohr mcp");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = BufReader::new(child.stdout.take().expect("stdout"));
    (child, stdin, stdout)
}

/// Waits up to `limit` for `child` to exit; returns its exit code and how long
/// it took. Kills it and fails if it outlives `limit`.
fn exits_within(child: &mut Child, limit: Duration) -> (Option<i32>, Duration) {
    let start = Instant::now();
    while start.elapsed() < limit {
        if let Some(status) = child.try_wait().expect("poll the child") {
            return (status.code(), start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("fernrohr mcp still running after {limit:?}");
}

#[test]
fn closing_stdin_ends_the_command_within_a_second() {
    let home = FreshHome::new();
    let (mut child, mut stdin, mut stdout) = spawn_mcp(&home);
    handshake(&mut stdin, &mut stdout);

    drop(stdin);
    let (code, took) = exits_within(&mut child, Duration::from_secs(1));
    eprintln!("stdin closed: exited in {took:?}");
    assert_eq!(code, Some(0));
}

#[test]
fn sigterm_ends_the_command_within_a_second_with_stdin_open() {
    let home = FreshHome::new();
    let (mut child, mut stdin, mut stdout) = spawn_mcp(&home);
    handshake(&mut stdin, &mut stdout);

    let sent = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("run kill");
    assert!(sent.success());
    let (code, took) = exits_within(&mut child, Duration::from_secs(1));
    eprintln!("SIGTERM: exited in {took:?}");
    // Exited through its handler (128 + SIGTERM), not killed by the signal.
    assert_eq!(code, Some(143));
    drop(stdin);
}

/// Kills the orphaned adapter if the test fails before it exits on its own.
struct KillOnDrop(String);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-KILL", &self.0])
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn the_parent_dying_ends_the_command_within_two_seconds() {
    let home = FreshHome::new();
    // An intermediate shell launches the adapter in the background on a copy
    // of the shell's stdin (a background job otherwise reads /dev/null),
    // prints its pid, and waits. The test kills the shell, so the adapter is
    // orphaned with stdin still open and no signal of its own.
    let mut shell = Command::new("sh")
        .args([
            "-c",
            r#"exec 3<&0; "$0" mcp <&3 3<&- 2>/dev/null & echo $!; wait"#,
            env!("CARGO_BIN_EXE_fernrohr"),
        ])
        .envs(home.vars())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the intermediate shell");
    let mut stdin = shell.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(shell.stdout.take().expect("stdout"));
    let mut pid = String::new();
    stdout.read_line(&mut pid).expect("read the adapter's pid");
    let _cleanup = KillOnDrop(pid.trim().to_string());
    handshake(&mut stdin, &mut stdout);

    shell.kill().expect("kill the intermediate shell");
    shell.wait().expect("reap the intermediate shell");
    let start = Instant::now();
    // The adapter is the last process holding stdout's write end, so EOF on it
    // means the adapter has exited.
    let (done, exited) = mpsc::channel();
    std::thread::spawn(move || {
        let mut rest = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut rest);
        let _ = done.send(());
    });
    exited
        .recv_timeout(Duration::from_secs(2))
        .expect("fernrohr mcp still running 2 s after its parent died");
    eprintln!("parent died: exited in {:?}", start.elapsed());
    drop(stdin);
}
