//! [`CommandTransport`]: the [`ForwardTransport`] a command tunnel plugs into
//! `ForwardSupervisor`, so it gets the same connect / health-check / reconnect-with-
//! backoff cycle an SSH tunnel does.
//!
//! - **connect** stops whatever ran before, checks the local port is free (a port
//!   already answering would read as "ready" when it's someone else's), runs the
//!   command with the login `PATH`, and polls the port until it accepts a connection,
//!   the command exits, the startup timeout passes, or the command sits at a prompt
//!   no one will answer (#423). Each failure carries the command's recent output.
//!   Until the tunnel has been ready once, what happens next depends on whether
//!   starting again can help ([`Startup`]): a prompt, the full startup timeout or a
//!   program that won't start gives up at once, so the connection waiting on it
//!   hears why instead of waiting while the command stops at the same login again.
//!   An exit or a busy port - the network not up yet, a previous run still letting
//!   go of the port - is retried, up to [`COMMAND_TUNNEL_STARTUP_ATTEMPTS`] times in
//!   a row. Once the tunnel has been up, every failure is retried with backoff, as a
//!   dropped session usually comes back.
//! - **health** is "still running, and the port still answers".
//! - **stop** (on reconnect and on drop) ends the whole process group.

use super::output::OutputTail;
use super::process::{self, Running};
use crate::consts::{
    COMMAND_TUNNEL_POLL_INTERVAL, COMMAND_TUNNEL_PROMPT_LINE_QUIET, COMMAND_TUNNEL_PROMPT_QUIET,
    COMMAND_TUNNEL_STARTUP_ATTEMPTS,
};
use crate::forward::supervisor::{ConnectFailure, ForwardTransport};
use std::time::Duration;
use tokio::net::TcpStream;

/// Why a start failed, by whether starting again can help - what decides, before
/// the tunnel has been ready, between retrying and giving up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Startup {
    /// Starting again would end the same way: a prompt no one will answer, the full
    /// startup timeout already waited out, a program that won't start.
    Settled,
    /// May pass by itself: the command exited (the network not up yet, a backend
    /// blip), or the port is still held (a previous run letting go of it).
    Transient,
}

pub(crate) struct CommandTransport {
    /// The command's arguments, `{port}` already substituted.
    argv: Vec<String>,
    local_port: u16,
    startup_timeout: Duration,
    tail: OutputTail,
    running: Option<Running>,
    /// Overrides the login `PATH`, for tests.
    path: Option<String>,
    /// Where the running command's pid is recorded for the owning tunnel.
    pid_slot: crate::util::pidfile::PidSlot,
    /// Whether the port has answered since this transport was made: before then, a
    /// failure is the command's setup failing, and is retried only if it's
    /// [`Startup::Transient`].
    been_ready: bool,
    /// Transient failures in a row before the first ready.
    transient_failures: u32,
}

impl CommandTransport {
    pub(crate) fn new(argv: Vec<String>, local_port: u16, startup_timeout: Duration) -> Self {
        Self {
            argv,
            local_port,
            startup_timeout,
            tail: OutputTail::default(),
            running: None,
            path: None,
            pid_slot: crate::util::pidfile::PidSlot::default(),
            been_ready: false,
            transient_failures: 0,
        }
    }

    /// Records the running command's pid in `slot`, while it runs.
    pub(crate) fn with_pid_slot(mut self, slot: crate::util::pidfile::PidSlot) -> Self {
        self.pid_slot = slot;
        self
    }

    /// The running command's pid, for tests to signal it directly.
    #[cfg(test)]
    pub(crate) fn pid(&self) -> Option<u32> {
        self.running.as_ref().and_then(|running| running.child.id())
    }

    /// Stops the command, if one is running, and waits for its group to be gone.
    pub(crate) async fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop().await;
        }
    }

    async fn login_path(&self) -> String {
        if let Some(path) = &self.path {
            return path.clone();
        }
        tokio::task::spawn_blocking(crate::util::login_env::login_path)
            .await
            .map(str::to_string)
            .unwrap_or_default()
    }

    async fn port_answers(&self) -> bool {
        TcpStream::connect(("127.0.0.1", self.local_port))
            .await
            .is_ok()
    }

    /// What a start failing for `reason` leads to: once the tunnel has been ready, a
    /// retry; before, giving up - at once if it's [`Startup::Settled`], after
    /// [`COMMAND_TUNNEL_STARTUP_ATTEMPTS`] in a row if it's transient.
    fn startup_failure(&mut self, kind: Startup, reason: String) -> ConnectFailure {
        if self.been_ready {
            return ConnectFailure::Retry(reason);
        }
        match kind {
            Startup::Settled => ConnectFailure::GiveUp(reason),
            Startup::Transient => {
                self.transient_failures += 1;
                if self.transient_failures >= COMMAND_TUNNEL_STARTUP_ATTEMPTS {
                    ConnectFailure::GiveUp(reason)
                } else {
                    ConnectFailure::Retry(reason)
                }
            }
        }
    }

    /// Polls until the port answers (`Ok`), or the command exits, sits at a prompt
    /// or the timeout passes (`Err`, the group stopped).
    async fn wait_until_ready(&mut self) -> Result<(), ConnectFailure> {
        let deadline = tokio::time::Instant::now() + self.startup_timeout;
        loop {
            if self.port_answers().await {
                self.been_ready = true;
                self.transient_failures = 0;
                return Ok(());
            }
            let Some(running) = self.running.as_mut() else {
                return Err(self
                    .startup_failure(Startup::Settled, "the command is not running".to_string()));
            };
            match running.child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => {
                    running.finish_output().await;
                    let reason =
                        format!("the command exited before the tunnel was ready ({status})");
                    self.stop().await;
                    let reason = self.tail.with_reason(reason);
                    return Err(self.startup_failure(Startup::Transient, reason));
                }
                Err(error) => {
                    self.stop().await;
                    let reason = format!("failed to check the command: {error}");
                    return Err(self.startup_failure(Startup::Settled, reason));
                }
            }
            if let Some(prompt) = self.tail.waiting_prompt(
                COMMAND_TUNNEL_PROMPT_QUIET,
                COMMAND_TUNNEL_PROMPT_LINE_QUIET,
            ) {
                self.stop().await;
                let reason = format!(
                    "the command is waiting for input (\"{prompt}\"), which Fernrohr can't \
                     answer. Run the command once in a terminal and answer it there - sign \
                     in, accept the host key or add the key to your SSH agent - then connect \
                     again."
                );
                let reason = self.tail.with_reason(reason);
                return Err(self.startup_failure(Startup::Settled, reason));
            }
            if tokio::time::Instant::now() >= deadline {
                self.stop().await;
                let reason = format!(
                    "timed out after {}s waiting for the command to listen on 127.0.0.1:{}. \
                     If it asks for input, run it once in a terminal first.",
                    self.startup_timeout.as_secs(),
                    self.local_port
                );
                let reason = self.tail.with_reason(reason);
                return Err(self.startup_failure(Startup::Settled, reason));
            }
            tokio::time::sleep(COMMAND_TUNNEL_POLL_INTERVAL).await;
        }
    }

    /// Starts the command and waits for it to be ready - [`ForwardTransport`]'s
    /// `connect`, saying whether a failure may be retried.
    async fn start(&mut self) -> Result<(), ConnectFailure> {
        self.stop().await;
        self.tail.clear();
        if let Err(error) = std::net::TcpListener::bind(("127.0.0.1", self.local_port)) {
            let reason = match error.kind() {
                std::io::ErrorKind::AddrInUse => {
                    format!("local port {} is already in use", self.local_port)
                }
                _ => format!("can't use local port {}: {error}", self.local_port),
            };
            return Err(self.startup_failure(Startup::Transient, reason));
        }
        let path = self.login_path().await;
        match process::spawn(&self.argv, &path, &self.tail, &self.pid_slot) {
            Ok(running) => self.running = Some(running),
            Err(reason) => return Err(self.startup_failure(Startup::Settled, reason)),
        }
        self.wait_until_ready().await
    }
}

impl ForwardTransport for CommandTransport {
    async fn connect(&mut self) -> Result<(), String> {
        self.start().await.map_err(|failure| match failure {
            ConnectFailure::Retry(reason) | ConnectFailure::GiveUp(reason) => reason,
        })
    }

    async fn connect_outcome(&mut self) -> Result<(), ConnectFailure> {
        self.start().await
    }

    async fn health_check(&mut self) -> Result<(), String> {
        let Some(running) = self.running.as_mut() else {
            return Err("the command is not running".to_string());
        };
        if let Ok(Some(status)) = running.child.try_wait() {
            running.finish_output().await;
            return Err(self
                .tail
                .with_reason(format!("the command exited ({status})")));
        }
        if !self.port_answers().await {
            return Err(format!(
                "the command stopped answering on 127.0.0.1:{}",
                self.local_port
            ));
        }
        Ok(())
    }
}

impl Drop for CommandTransport {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop_in_background();
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
