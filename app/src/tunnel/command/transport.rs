//! [`CommandTransport`]: the [`ForwardTransport`] a command tunnel plugs into
//! `ForwardSupervisor`, so it gets the same connect / health-check / reconnect-with-
//! backoff cycle an SSH tunnel does.
//!
//! - **connect** stops whatever ran before, checks the local port is free (a port
//!   already answering would read as "ready" when it's someone else's), runs the
//!   command with the login `PATH`, and polls the port until it accepts a connection,
//!   the command exits, the startup timeout passes, or the command sits at a prompt
//!   no one will answer (#423). Each failure carries the command's recent output.
//!   Until the tunnel has been ready once, a failure gives up rather than retrying:
//!   running the command again would only stop at the same login or prompt, while
//!   the connection waiting on it never heard why. Once it has been up, a failure
//!   is retried with backoff, as a dropped session usually comes back.
//! - **health** is "still running, and the port still answers".
//! - **stop** (on reconnect and on drop) ends the whole process group.

use super::output::OutputTail;
use super::process::{self, Running};
use crate::consts::{COMMAND_TUNNEL_POLL_INTERVAL, COMMAND_TUNNEL_PROMPT_QUIET};
use crate::forward::supervisor::{ConnectFailure, ForwardTransport};
use std::time::Duration;
use tokio::net::TcpStream;

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
    /// Whether the port has answered since this transport was made: a failure
    /// before then is the command's setup failing, and isn't retried.
    been_ready: bool,
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

    /// A startup failure for `reason`: given up on until the tunnel has been ready
    /// once, retried after.
    fn startup_failure(&self, reason: String) -> ConnectFailure {
        if self.been_ready {
            ConnectFailure::Retry(reason)
        } else {
            ConnectFailure::GiveUp(reason)
        }
    }

    /// Polls until the port answers (`Ok`), or the command exits, sits at a prompt
    /// or the timeout passes (`Err`, the group stopped).
    async fn wait_until_ready(&mut self) -> Result<(), ConnectFailure> {
        let deadline = tokio::time::Instant::now() + self.startup_timeout;
        loop {
            if self.port_answers().await {
                self.been_ready = true;
                return Ok(());
            }
            let Some(running) = self.running.as_mut() else {
                return Err(self.startup_failure("the command is not running".to_string()));
            };
            match running.child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => {
                    running.finish_output().await;
                    let reason =
                        format!("the command exited before the tunnel was ready ({status})");
                    self.stop().await;
                    return Err(self.startup_failure(self.tail.with_reason(reason)));
                }
                Err(error) => {
                    self.stop().await;
                    return Err(
                        self.startup_failure(format!("failed to check the command: {error}"))
                    );
                }
            }
            if let Some(prompt) = self.tail.waiting_prompt(COMMAND_TUNNEL_PROMPT_QUIET) {
                self.stop().await;
                let reason = format!(
                    "the command is waiting for input (\"{prompt}\"), which Fernrohr can't \
                     answer. Run the command once in a terminal and answer it there - sign \
                     in, accept the host key or add the key to your SSH agent - then connect \
                     again."
                );
                return Err(self.startup_failure(self.tail.with_reason(reason)));
            }
            if tokio::time::Instant::now() >= deadline {
                self.stop().await;
                let reason = format!(
                    "timed out after {}s waiting for the command to listen on 127.0.0.1:{}. \
                     If it asks for input, run it once in a terminal first.",
                    self.startup_timeout.as_secs(),
                    self.local_port
                );
                return Err(self.startup_failure(self.tail.with_reason(reason)));
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
            return Err(self.startup_failure(reason));
        }
        let path = self.login_path().await;
        let running = process::spawn(&self.argv, &path, &self.tail, &self.pid_slot)
            .map_err(|reason| self.startup_failure(reason))?;
        self.running = Some(running);
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
