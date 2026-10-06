//! The exec transport over in-memory pipes standing in for a cluster's exec:
//! output reaches the terminal, input reaches stdin, only the settled size is
//! sent, the exit status is read, and dropping the transport ends the pump.

use super::{ExecTransport, exit_of, pump};
use gpui_terminal::{GridSize, Terminal, TerminalBuilder, TerminalSink, Transport};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status;
use kube::api::TerminalSize;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::{mpsc, watch};

/// A transport that goes nowhere: the tests drive the sink directly.
struct Nowhere;

impl Transport for Nowhere {
    fn write(&mut self, _bytes: &[u8]) -> gpui_terminal::Result<()> {
        Ok(())
    }
    fn resize(&mut self, _size: GridSize) -> gpui_terminal::Result<()> {
        Ok(())
    }
    fn terminate(&mut self) -> gpui_terminal::Result<()> {
        Ok(())
    }
}

/// A terminal, and the sink its output arrives through.
fn terminal() -> (Terminal<Nowhere>, TerminalSink) {
    let mut sink = None;
    let terminal = TerminalBuilder::new()
        .connect(|handed| {
            sink = Some(handed);
            Ok::<_, std::convert::Infallible>(Nowhere)
        })
        .expect("connects");
    (terminal, sink.expect("connect hands over a sink"))
}

fn size(columns: usize, rows: usize) -> GridSize {
    GridSize { columns, rows }
}

/// A pump over pipes: what the "container" writes to stdout, and reads from
/// stdin, are the test's ends. Sizes sent are recorded.
struct Pipes {
    stdout: tokio::io::DuplexStream,
    stdin: tokio::io::DuplexStream,
    input: mpsc::Sender<Vec<u8>>,
    resize: watch::Sender<Option<GridSize>>,
    sizes: Arc<Mutex<Vec<TerminalSize>>>,
    pump: tokio::task::JoinHandle<()>,
}

fn pipes(sink: TerminalSink) -> Pipes {
    let (stdout, stdout_far) = tokio::io::duplex(4096);
    let (stdin, stdin_far) = tokio::io::duplex(4096);
    let (input, input_rx) = mpsc::channel(8);
    let (resize, resize_rx) = watch::channel(None);
    let sizes = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&sizes);
    let pump = tokio::spawn(async move {
        pump(
            Some(stdout_far),
            Some(stdin_far),
            input_rx,
            resize_rx,
            move |size| recorded.lock().push(size),
            &sink,
        )
        .await;
    });
    Pipes {
        stdout,
        stdin,
        input,
        resize,
        sizes,
        pump,
    }
}

#[tokio::test]
async fn output_reaches_the_terminal_and_input_reaches_stdin() {
    let (terminal, sink) = terminal();
    let mut pipes = pipes(sink);

    pipes.stdout.write_all(b"$ hello").await.unwrap();
    pipes.input.send(b"ls\r".to_vec()).await.unwrap();
    let mut typed = [0u8; 3];
    tokio::time::timeout(Duration::from_secs(5), pipes.stdin.read_exact(&mut typed))
        .await
        .expect("stdin got the input in time")
        .unwrap();
    assert_eq!(&typed, b"ls\r");

    // Stdout ending ends the pump; by then the output is in the grid.
    drop(pipes.stdout);
    tokio::time::timeout(Duration::from_secs(5), pipes.pump)
        .await
        .expect("the pump ends with stdout")
        .unwrap();
    assert_eq!(terminal.with_grid(|grid| grid.row_text(0)), "$ hello");
}

#[tokio::test]
async fn only_the_size_resizing_settles_on_is_sent() {
    let (_terminal, sink) = terminal();
    let pipes = pipes(sink);

    for columns in [80, 90, 100, 120] {
        pipes.resize.send_replace(Some(size(columns, 30)));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(crate::consts::EXEC_RESIZE_DEBOUNCE * 3).await;

    let sizes: Vec<(u16, u16)> = pipes
        .sizes
        .lock()
        .iter()
        .map(|size| (size.width, size.height))
        .collect();
    assert_eq!(sizes, [(120, 30)], "one resize, the last");
}

#[tokio::test]
async fn dropping_the_transport_closes_stdin_and_stops_the_pump() {
    let (_terminal, sink) = terminal();
    let mut pipes = pipes(sink);
    // The transport owns the pump's task, as `spawn` makes it.
    let transport = ExecTransport::with_task(pipes.input.clone(), pipes.resize.clone(), pipes.pump);

    drop(transport);

    let mut rest = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), pipes.stdin.read_to_end(&mut rest))
        .await
        .expect("stdin closed in time");
    assert_eq!(read.unwrap(), 0, "stdin closed with nothing more written");
}

#[tokio::test]
async fn writes_queue_and_a_finished_session_refuses_them() {
    let (input, mut input_rx) = mpsc::channel(1);
    let (resize, mut resize_rx) = watch::channel(None);
    let mut transport =
        ExecTransport::with_task(input, resize, tokio::spawn(std::future::pending()));

    transport.write(b"a").expect("queued");
    assert!(
        matches!(
            transport.write(b"b"),
            Err(gpui_terminal::Error::Transport(_))
        ),
        "a full queue refuses rather than blocks"
    );
    assert_eq!(input_rx.recv().await.as_deref(), Some(&b"a"[..]));

    transport.resize(size(100, 40)).expect("recorded");
    assert_eq!(*resize_rx.borrow_and_update(), Some(size(100, 40)));

    drop(input_rx);
    assert!(matches!(
        transport.write(b"c"),
        Err(gpui_terminal::Error::Closed)
    ));
}

fn status(json: serde_json::Value) -> Status {
    serde_json::from_value(json).expect("a Status")
}

#[test]
fn a_successful_exit_is_code_zero() {
    let success = status(serde_json::json!({ "status": "Success"}));
    assert_eq!(exit_of(Some(&success)), (Some(0), None));
}

#[test]
fn a_non_zero_exit_carries_its_code() {
    let failed = status(serde_json::json!({
        "status": "Failure", "reason": "NonZeroExitCode",
        "message": "command terminated with non-zero exit code: error executing command [sh], exit code 2",
        "details": { "causes": [{ "reason": "ExitCode", "message": "2" }] }
    }));
    assert_eq!(exit_of(Some(&failed)), (Some(2), None));
}

#[test]
fn any_other_failure_is_a_reason() {
    let refused = status(serde_json::json!({
        "status": "Failure", "reason": "InternalError",
        "message": "container not found"
    }));
    assert_eq!(
        exit_of(Some(&refused)),
        (None, Some("container not found".into()))
    );
}

#[test]
fn no_status_is_a_lost_connection() {
    let (code, reason) = exit_of(None);
    assert_eq!(code, None);
    assert!(reason.is_some_and(|reason| reason.contains("connection")));
}

/// Output streaming the whole time doesn't hold a resize back: the size is
/// sent once resizing settles, however busy the program is.
#[tokio::test]
async fn a_busy_program_still_gets_its_new_size() {
    let (_terminal, sink) = terminal();
    let mut pipes = pipes(sink);

    pipes.resize.send_replace(Some(size(132, 40)));
    let deadline = tokio::time::Instant::now() + crate::consts::EXEC_RESIZE_DEBOUNCE * 4;
    while tokio::time::Instant::now() < deadline {
        pipes.stdout.write_all(b"tick\r\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let sizes: Vec<(u16, u16)> = pipes
        .sizes
        .lock()
        .iter()
        .map(|size| (size.width, size.height))
        .collect();
    assert_eq!(sizes, [(132, 40)]);
}

/// Output ended but the status never comes - an unclean drop: past the limit
/// the session ends as a lost connection, the terminal stops taking input,
/// and the panel hears it, rather than staying "running" forever.
#[tokio::test]
async fn a_status_that_never_arrives_ends_as_a_lost_connection() {
    let (terminal, sink) = terminal();
    let (ended, mut ended_rx) = mpsc::channel(1);
    let never = std::future::pending::<Option<Status>>();

    tokio::time::timeout(
        Duration::from_secs(5),
        super::finish_session(Some(never), Duration::from_millis(50), &sink, &ended),
    )
    .await
    .expect("the wait for a status is bounded");

    let end = ended_rx.try_recv().expect("the panel was told");
    assert_eq!(end.code, None);
    assert!(
        end.reason
            .is_some_and(|reason| reason.contains("connection")),
        "a lost connection"
    );
    assert!(
        terminal.exit_report().is_some(),
        "the terminal heard the exit"
    );
}

/// A status that does arrive in time is the exit reported.
#[tokio::test]
async fn a_status_in_time_is_the_exit() {
    let (_terminal, sink) = terminal();
    let (ended, mut ended_rx) = mpsc::channel(1);
    let success = status(serde_json::json!({ "status": "Success" }));

    super::finish_session(
        Some(async move { Some(success) }),
        Duration::from_secs(5),
        &sink,
        &ended,
    )
    .await;

    assert_eq!(ended_rx.try_recv().map(|end| end.code).ok(), Some(Some(0)));
}
