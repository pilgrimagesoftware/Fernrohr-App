//! #423: a command stuck at a prompt no one will answer fails with the prompt as
//! its reason and is stopped, and a startup failure before the tunnel was ever
//! ready gives up rather than retrying - so a connection waiting on it hears why.

use super::*;

/// A prompt written without a newline, as ssh writes one, fails as waiting for
/// input - well before the timeout - and the command's whole group is stopped.
#[tokio::test]
async fn a_prompt_fails_before_the_timeout_and_stops_the_command() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport("prompt", port, Duration::from_secs(120));
    let path = transport.login_path().await;
    transport.running = Some(
        process::spawn(&transport.argv, &path, &transport.tail, &transport.pid_slot)
            .expect("started"),
    );
    let pid = transport.pid().expect("running");
    let failure = transport
        .wait_until_ready()
        .await
        .expect_err("waits for input");
    let ConnectFailure::GiveUp(reason) = failure else {
        panic!("a prompt before the tunnel was ready gives up: {failure:?}");
    };
    assert!(reason.contains("waiting for input"), "{reason}");
    assert!(
        reason.contains("Enter passphrase for key '/home/user/.ssh/id_ed25519':"),
        "the prompt is quoted: {reason}"
    );
    assert!(!reason.contains("timed out"), "{reason}");
    assert!(transport.pid().is_none(), "the command was stopped");
    assert!(
        eventually(|| !process_group_alive(pid)).await,
        "its process group is gone"
    );
}

/// Some tools end their prompt with a newline; the last line still counts.
#[tokio::test]
async fn a_prompt_on_its_own_line_fails_too() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport("hang", port, Duration::from_secs(120));
    let reason = transport.connect().await.expect_err("waits for input");
    assert!(reason.contains("waiting for input"), "{reason}");
    assert!(reason.contains("Enter passphrase for key:"), "{reason}");
}

/// A command that never listens - stuck at a prompt on the terminal, which its
/// output never shows - gives up at the timeout instead of starting again.
#[tokio::test]
async fn a_timeout_before_the_tunnel_was_ready_gives_up() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport("silent", port, Duration::from_secs(1));
    let failure = transport.connect_outcome().await.expect_err("times out");
    assert!(
        matches!(&failure, ConnectFailure::GiveUp(reason) if reason.contains("timed out")),
        "{failure:?}"
    );
}

/// Once the tunnel has been up, a failure to start again is retried: a dropped
/// session, or a port someone holds for a moment, usually comes back.
#[tokio::test]
async fn after_being_ready_a_startup_failure_is_retried() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let mut transport = transport(&format!("listen:{port}"), port, Duration::from_secs(20));
    transport.connect().await.expect("ready");
    transport.stop().await;
    assert!(eventually(|| port_is_free(port)).await);
    let _squatter = std::net::TcpListener::bind(("127.0.0.1", port)).expect("bound");
    let failure = transport.connect_outcome().await.expect_err("port in use");
    assert!(
        matches!(&failure, ConnectFailure::Retry(reason) if reason.contains("already in use")),
        "{failure:?}"
    );
}

/// Under the supervisor, a prompt ends the forward - its state channel closes
/// without ever reporting Up - and leaves the prompt as the reason, which the
/// connection waiting on it reports.
#[tokio::test]
async fn the_supervisor_gives_up_with_the_prompt_as_its_reason() {
    let _serial = PORT_TESTS.lock().await;
    let port = free_port();
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let transport = transport("prompt", port, Duration::from_secs(120));
    let options = SupervisorOptions {
        health_check_interval: Duration::from_millis(100),
        backoff: BackoffPolicy {
            initial: Duration::from_millis(50),
            max: Duration::from_millis(200),
        },
    };
    let supervisor = ForwardSupervisor::spawn(&Handle::current(), addr, transport, options);
    let mut state = supervisor.state();
    let closed = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            assert_ne!(*state.borrow_and_update(), ForwardState::Up);
            if state.changed().await.is_err() {
                break;
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the forward gave up");
    let reason = supervisor.failure().get().expect("a reason was left");
    assert!(reason.contains("waiting for input"), "{reason}");
}
