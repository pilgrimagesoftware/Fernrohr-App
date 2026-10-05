//! [`CommandTransport`]: the [`ForwardTransport`] a command tunnel plugs into
//! `ForwardSupervisor`, so it gets the same connect / health-check / reconnect-with-
//! backoff cycle an SSH tunnel does.
//!
//! - **connect** stops whatever ran before, checks the local port is free (a port
//!   already answering would read as "ready" when it's someone else's), runs the
//!   command with the login `PATH`, and polls the port until it accepts a connection,
//!   the command exits, or the startup timeout passes. Each failure carries the
//!   command's recent output.
//! - **health** is "still running, and the port still answers".
//! - **stop** (on reconnect and on drop) ends the whole process group.

use super::output::OutputTail;
use super::process::{self, Running};
use crate::consts::COMMAND_TUNNEL_POLL_INTERVAL;
use crate::forward::supervisor::ForwardTransport;
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
        }
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

    /// Polls until the port answers (`Ok`), or the command exits or the timeout
    /// passes (`Err`, the group stopped).
    async fn wait_until_ready(&mut self) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + self.startup_timeout;
        loop {
            if self.port_answers().await {
                return Ok(());
            }
            let Some(running) = self.running.as_mut() else {
                return Err("the command is not running".to_string());
            };
            match running.child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => {
                    running.finish_output().await;
                    let reason =
                        format!("the command exited before the tunnel was ready ({status})");
                    self.stop().await;
                    return Err(self.tail.with_reason(reason));
                }
                Err(error) => {
                    self.stop().await;
                    return Err(format!("failed to check the command: {error}"));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                self.stop().await;
                let reason = format!(
                    "timed out after {}s waiting for the command to listen on 127.0.0.1:{}. \
                     If it asks for input, run it once in a terminal first.",
                    self.startup_timeout.as_secs(),
                    self.local_port
                );
                return Err(self.tail.with_reason(reason));
            }
            tokio::time::sleep(COMMAND_TUNNEL_POLL_INTERVAL).await;
        }
    }
}

impl ForwardTransport for CommandTransport {
    async fn connect(&mut self) -> Result<(), String> {
        self.stop().await;
        self.tail.clear();
        if let Err(error) = std::net::TcpListener::bind(("127.0.0.1", self.local_port)) {
            return Err(match error.kind() {
                std::io::ErrorKind::AddrInUse => {
                    format!("local port {} is already in use", self.local_port)
                }
                _ => format!("can't use local port {}: {error}", self.local_port),
            });
        }
        let path = self.login_path().await;
        self.running = Some(process::spawn(&self.argv, &path, &self.tail)?);
        self.wait_until_ready().await
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
