//! Why `fernrohr mcp` stops, and the watch for a client that went away without
//! closing stdin (cli-help-and-shutdown D3): a terminating signal, or the
//! parent process exiting so the adapter is reparented.
//!
//! Stdin EOF needs nothing here: rmcp's transport closes and `serve` returns.

use crate::consts;
use tokio::signal::unix::{Signal, SignalKind, signal};

/// Why the adapter stopped serving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::mcp) enum Stop {
    /// The client closed the transport.
    Closed,
    /// A terminating signal arrived; carries its number.
    Signal(i32),
    /// The process that launched the adapter exited.
    ParentGone,
}

impl Stop {
    /// The process exit code: 0 when the client went away, 128 plus the
    /// signal number when it was signalled, as a shell reports it.
    pub(in crate::mcp) fn exit_code(self) -> i32 {
        match self {
            Self::Closed | Self::ParentGone => 0,
            Self::Signal(number) => 128 + number,
        }
    }
}

/// Resolves when the client is gone without having closed stdin: on SIGTERM,
/// SIGINT or SIGHUP, or once the parent process changes from `parent`.
pub(in crate::mcp) async fn client_gone(parent: u32) -> Stop {
    let mut terminate = listen(SignalKind::terminate());
    let mut interrupt = listen(SignalKind::interrupt());
    let mut hangup = listen(SignalKind::hangup());
    tokio::select! {
        stop = received(&mut terminate, SignalKind::terminate()) => stop,
        stop = received(&mut interrupt, SignalKind::interrupt()) => stop,
        stop = received(&mut hangup, SignalKind::hangup()) => stop,
        () = parent_gone(parent) => Stop::ParentGone,
    }
}

/// A stream for `kind`. If it can't be installed, the adapter still stops on
/// the other signals, the parent watch and stdin EOF.
fn listen(kind: SignalKind) -> Option<Signal> {
    signal(kind)
        .inspect_err(|error| {
            eprintln!(
                "fernrohr mcp: can't watch signal {}: {error}",
                kind.as_raw_value()
            );
        })
        .ok()
}

async fn received(stream: &mut Option<Signal>, kind: SignalKind) -> Stop {
    match stream {
        Some(stream) => {
            stream.recv().await;
            Stop::Signal(kind.as_raw_value())
        }
        None => std::future::pending().await,
    }
}

/// Resolves once this process's parent is no longer `parent`. A process
/// whose parent exits is reparented, to init or a subreaper, so polling the
/// parent id is enough and costs one syscall per `MCP_PARENT_POLL`.
async fn parent_gone(parent: u32) {
    let mut poll = tokio::time::interval(consts::MCP_PARENT_POLL);
    loop {
        poll.tick().await;
        if std::os::unix::process::parent_id() != parent {
            return;
        }
    }
}
