//! `command-tunnels` 2.2-2.3 against real processes. The stand-in command is this
//! test binary itself, re-run with `--exact` on [`command_tunnel_helper`] and a
//! `fernrohr-helper=<mode>` argument naming what to do - so the tests need no
//! listener tool on the machine, and run the same on macOS and Linux CI.

use super::*;
use crate::forward::managed::ForwardState;
use crate::forward::supervisor::{BackoffPolicy, ForwardSupervisor, SupervisorOptions};
use crate::util::pidfile::{kill_process_group, process_group_alive};
use std::net::SocketAddr;
use tokio::runtime::Handle;

const HELPER: &str = "tunnel::command::transport::tests::command_tunnel_helper";

/// The stand-in: does nothing on an ordinary run (it's ignored, and without a mode
/// returns at once); re-run by [`helper_argv`], acts out its mode.
#[test]
#[ignore = "a stand-in process for the command tunnel tests, run only by them"]
fn command_tunnel_helper() {
    let Some(mode) =
        std::env::args().find_map(|arg| arg.strip_prefix("fernrohr-helper=").map(str::to_string))
    else {
        return;
    };
    let (mode, port) = mode.split_once(':').unwrap_or((mode.as_str(), "0"));
    match mode {
        "listen" => {
            let listener = std::net::TcpListener::bind(("127.0.0.1", port.parse::<u16>().unwrap()))
                .expect("the helper binds its port");
            println!("listening on {port}");
            for stream in listener.incoming() {
                drop(stream);
            }
        }
        "spawn-listen" => {
            // A wrapper: the listener is a grandchild, in the same process group.
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(helper_args(&format!("listen:{port}")))
                .spawn()
                .expect("the helper spawns its listener");
            println!("started a listener on {port}");
            let _ = child.wait();
        }
        "exit" => {
            eprintln!("ERROR: (gcloud) You do not currently have an active account selected.");
            std::process::exit(3);
        }
        "hang" => {
            println!("Enter passphrase for key:");
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        other => panic!("unknown helper mode {other}"),
    }
}

fn helper_args(mode: &str) -> Vec<String> {
    [
        HELPER,
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    .into_iter()
    .map(str::to_string)
    .chain([format!("fernrohr-helper={mode}")])
    .collect()
}

fn helper_argv(mode: &str) -> Vec<String> {
    std::iter::once(std::env::current_exe().unwrap().display().to_string())
        .chain(helper_args(mode))
        .collect()
}

/// Held for the whole of each test that runs a command on a port. A test that
/// kills its command frees the port until the re-run takes it back, and a parallel
/// test's `free_port` could be handed it in between - so these run one at a time.
static PORT_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn free_port() -> u16 {
    crate::util::port_allocator::allocate()
        .unwrap()
        .addr()
        .port()
}

fn transport(mode: &str, port: u16, timeout: Duration) -> CommandTransport {
    let mut transport = CommandTransport::new(helper_argv(mode), port, timeout);
    transport.path = Some(std::env::var("PATH").unwrap_or_default());
    transport
}

async fn eventually(condition: impl Fn() -> bool) -> bool {
    for _ in 0..200 {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// 2.2: ready once the command listens on its port.
#[tokio::test]
async fn ready_when_the_port_listens() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport(&format!("listen:{port}"), port, Duration::from_secs(20));
    transport.connect().await.expect("ready");
    assert!(transport.health_check().await.is_ok());
    transport.stop().await;
    assert!(eventually(|| port_is_free(port)).await);
}

/// 2.2: a command that exits first fails with its status and its output.
#[tokio::test]
async fn exiting_before_ready_fails_with_the_output() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport("exit", port, Duration::from_secs(20));
    let reason = transport.connect().await.expect_err("exits");
    assert!(
        reason.contains("exited before the tunnel was ready"),
        "{reason}"
    );
    assert!(
        reason.contains("active account"),
        "the output tail: {reason}"
    );
}

/// 2.2: a command that never listens is stopped at the timeout, the failure
/// quoting what it printed (often the prompt it is stuck on).
#[tokio::test]
async fn a_command_that_never_listens_times_out_and_is_stopped() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport("hang", port, Duration::from_secs(2));
    let reason = transport.connect().await.expect_err("times out");
    assert!(reason.contains("timed out after 2s"), "{reason}");
    assert!(
        reason.contains("Enter passphrase"),
        "the output tail: {reason}"
    );
    assert!(transport.pid().is_none(), "the command was stopped");
}

/// A fixed port someone else already holds fails as in use, rather than reading
/// their listener as this command being ready.
#[tokio::test]
async fn a_port_already_in_use_fails_without_starting_the_command() {
    let _serial = PORT_TESTS.lock().await;
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let mut transport = transport(&format!("listen:{port}"), port, Duration::from_secs(5));
    let reason = transport.connect().await.expect_err("in use");
    assert!(reason.contains("already in use"), "{reason}");
    assert!(transport.pid().is_none());
}

/// 2.3: releasing the transport ends the whole group - the wrapper and the
/// grandchild holding the port - and frees the port.
#[tokio::test]
async fn dropping_ends_the_wrapper_and_its_grandchild() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport(
        &format!("spawn-listen:{port}"),
        port,
        Duration::from_secs(20),
    );
    transport.connect().await.expect("the grandchild listens");
    let pid = transport.pid().expect("running");
    drop(transport);
    assert!(
        eventually(|| port_is_free(port)).await,
        "the grandchild's port is freed"
    );
    assert!(
        eventually(|| !process_group_alive(pid)).await,
        "the whole group is gone"
    );
}

/// 2.3: a command killed while Up drives the supervisor to Reconnecting, and a
/// re-run brings it back Up on the same port.
#[tokio::test]
async fn a_killed_command_reconnects_on_the_same_port() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let transport = transport(&format!("listen:{port}"), port, Duration::from_secs(20));
    let options = SupervisorOptions {
        health_check_interval: Duration::from_millis(100),
        backoff: BackoffPolicy {
            initial: Duration::from_millis(50),
            max: Duration::from_millis(200),
        },
    };
    let supervisor = ForwardSupervisor::spawn(&Handle::current(), addr, transport, options);
    let mut state = supervisor.state();
    wait_for(&mut state, ForwardState::Up).await;
    let pid = listening_pid(port).await;

    kill_process_group(pid);
    wait_for(&mut state, ForwardState::Reconnecting).await;
    wait_for(&mut state, ForwardState::Up).await;
    assert_eq!(supervisor.local_addr(), addr);
    assert!(
        !port_is_free(port),
        "the re-run command holds the same port"
    );
}

async fn wait_for(state: &mut tokio::sync::watch::Receiver<ForwardState>, target: ForwardState) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while *state.borrow_and_update() != target {
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {target:?}"));
}

/// The pid of this run's own command-tunnel stand-in listening on `port`, from its
/// pidfile-free twin: `ps` for the helper's `listen:<port>` argument.
async fn listening_pid(port: u16) -> u32 {
    let needle = format!("fernrohr-helper=listen:{port}");
    let output = tokio::process::Command::new("ps")
        .args([
            "-ww",
            "-o",
            "pid=,args=",
            "-u",
            &std::env::var("USER").unwrap_or_default(),
        ])
        .output()
        .await
        .expect("ps runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| line.contains(&needle))
        .and_then(|line| line.split_whitespace().next()?.parse().ok())
        .expect("the stand-in is running")
}

/// The stand-in as a stored command line, `{port}` left for the tunnel to fill.
fn helper_command_line(mode: &str) -> String {
    helper_argv(mode)
        .iter()
        .map(|arg| shlex::try_quote(arg).expect("quotable").into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether any stand-in - this test binary, re-run - with `needle` in its arguments
/// is still running. Only processes running this binary count, so a shell whose
/// command line merely mentions the needle doesn't.
async fn helper_running(needle: &str) -> bool {
    let exe = std::env::current_exe().unwrap().display().to_string();
    let output = tokio::process::Command::new("ps")
        .args([
            "-ww",
            "-o",
            "args=",
            "-u",
            &std::env::var("USER").unwrap_or_default(),
        ])
        .output()
        .await
        .expect("ps runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim_start().starts_with(&exe) && line.contains(needle))
}

/// 4.2: the Tunnels window's Test runs the command until it is ready, reports
/// success, and leaves nothing running.
#[tokio::test]
async fn test_command_succeeds_and_leaves_nothing_running() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let config = crate::config::tunnels::CommandTunnelConfig {
        command_line: helper_command_line("spawn-listen:{port}"),
        local_port: Some(port),
        startup_timeout_secs: 20,
        ..Default::default()
    };
    crate::tunnel::command::test_command(&config)
        .await
        .expect("reachable");
    let needle = format!("fernrohr-helper=spawn-listen:{port}");
    assert!(eventually(|| port_is_free(port)).await);
    for _ in 0..40 {
        if !helper_running(&needle).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("a stand-in from the test is still running");
}

/// 4.2: a failing Test reports the command's output and leaves nothing running.
#[tokio::test]
async fn a_failing_test_command_reports_the_output() {
    let _serial = PORT_TESTS.lock().await;
    let config = crate::config::tunnels::CommandTunnelConfig {
        command_line: format!("{} {{port}}", helper_command_line("exit")),
        startup_timeout_secs: 20,
        ..Default::default()
    };
    let reason = crate::tunnel::command::test_command(&config)
        .await
        .expect_err("the command exits");
    assert!(reason.contains("active account"), "{reason}");
    assert!(!helper_running("fernrohr-helper=exit").await);
}
