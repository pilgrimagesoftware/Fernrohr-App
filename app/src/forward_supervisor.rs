//! Section 2.3 of the tunnel-subsystem change: the per-forward supervisor task that
//! drives a `ManagedForward` implementation's [`ForwardState`](crate::managed_forward::ForwardState)
//! machine - connect, periodic health check, and reconnect-with-backoff on failure.
//!
//! `SshTunnel` and `K8sPortForward` (sections 3-4) each supply a [`ForwardTransport`]
//! (spawn/supervise an `ssh` child, or hold a `kube` port-forward stream) and let this
//! module own the retry policy so neither has to reimplement it.

use crate::managed_forward::ForwardState;
use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::watch;

/// What a `ManagedForward` implementation plugs in: how to establish the forward and
/// how to tell it's still alive. Both return a `String` reason on failure so the
/// supervisor's caller can surface it without the supervisor needing to know what
/// "ssh auth failed" or "pod deleted" means.
pub trait ForwardTransport: Send + 'static {
    fn connect(&mut self) -> impl Future<Output = Result<(), String>> + Send;
    fn health_check(&mut self) -> impl Future<Output = Result<(), String>> + Send;
}

/// How long to wait between reconnect attempts. Doubles per consecutive failure since
/// entering `Reconnecting`, capped at `max` - a bastion that's down for a while gets
/// backed off from, not hammered.
#[derive(Clone, Copy, Debug)]
pub struct BackoffPolicy {
    pub initial: Duration,
    pub max: Duration,
}

impl BackoffPolicy {
    /// `attempt` is 1 for the first failure since the last `Up`, 2 for the next, etc.
    fn delay(&self, attempt: u32) -> Duration {
        let scale = 1u32 << attempt.saturating_sub(1).min(16);
        self.initial.saturating_mul(scale).min(self.max)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SupervisorOptions {
    pub health_check_interval: Duration,
    pub backoff: BackoffPolicy,
}

/// Owns the spawned supervisor task; aborting it on drop per the project's "owning
/// spawned work" convention rather than leaking it.
// UNWIRED(#3): SshTunnel and K8sPortForward (sections 3-4) are the first real callers.
#[allow(dead_code)]
pub struct ForwardSupervisor {
    state_tx: watch::Sender<ForwardState>,
    local_addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl ForwardSupervisor {
    #[allow(dead_code)]
    pub fn spawn<T: ForwardTransport>(
        rt: &Handle,
        local_addr: SocketAddr,
        transport: T,
        options: SupervisorOptions,
    ) -> Self {
        let state_tx = watch::Sender::new(ForwardState::Disconnected);
        let task_state_tx = state_tx.clone();
        let task = rt.spawn(run(task_state_tx, transport, options));
        Self {
            state_tx,
            local_addr,
            task,
        }
    }

    #[allow(dead_code)]
    pub fn state(&self) -> watch::Receiver<ForwardState> {
        self.state_tx.subscribe()
    }

    #[allow(dead_code)]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Drop for ForwardSupervisor {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run<T: ForwardTransport>(
    state_tx: watch::Sender<ForwardState>,
    mut transport: T,
    options: SupervisorOptions,
) {
    let mut has_been_up = false;
    let mut attempt: u32 = 0;
    loop {
        state_tx.send_replace(if has_been_up {
            ForwardState::Reconnecting
        } else {
            ForwardState::Connecting
        });

        if let Err(_reason) = transport.connect().await {
            attempt += 1;
            tokio::time::sleep(options.backoff.delay(attempt)).await;
            continue;
        }

        has_been_up = true;
        attempt = 0;
        state_tx.send_replace(ForwardState::Up);

        loop {
            tokio::time::sleep(options.health_check_interval).await;
            if transport.health_check().await.is_err() {
                break;
            }
        }
    }
}

/// Exercises the retry state machine against a fake transport with a scripted
/// connect/health-check sequence - not coverage of `SshTunnel` or `K8sPortForward`
/// (sections 3-4), which don't exist yet.
#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[derive(Clone)]
    struct ScriptedTransport {
        /// Each entry is consumed in order by whichever call (`connect` or
        /// `health_check`) happens next; `Ok(())` succeeds, `Err(_)` fails once.
        /// Once exhausted, every further call repeats the last scripted result.
        queue: Arc<Mutex<std::collections::VecDeque<Result<(), String>>>>,
        last: Result<(), String>,
    }

    impl ScriptedTransport {
        fn new(script: Vec<Result<(), String>>) -> Self {
            let last = script.last().cloned().unwrap_or(Ok(()));
            Self {
                queue: Arc::new(Mutex::new(script.into())),
                last,
            }
        }

        fn next(&self) -> Result<(), String> {
            self.queue
                .lock()
                .pop_front()
                .unwrap_or_else(|| self.last.clone())
        }
    }

    impl ForwardTransport for ScriptedTransport {
        async fn connect(&mut self) -> Result<(), String> {
            // A real transport always has a genuine await point (spawning `ssh`,
            // awaiting a kube stream); yield here so a concurrently polling
            // watch::Receiver gets a chance to observe the state this call is
            // about to leave, instead of the whole reconnect cycle resolving
            // within one scheduler turn.
            tokio::task::yield_now().await;
            self.next()
        }

        async fn health_check(&mut self) -> Result<(), String> {
            tokio::task::yield_now().await;
            self.next()
        }
    }

    fn fast_options() -> SupervisorOptions {
        SupervisorOptions {
            health_check_interval: Duration::from_millis(5),
            backoff: BackoffPolicy {
                initial: Duration::from_millis(5),
                max: Duration::from_millis(20),
            },
        }
    }

    fn addr() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }

    async fn wait_for(state: &mut watch::Receiver<ForwardState>, target: ForwardState) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if *state.borrow() == target {
                    return;
                }
                state.changed().await.unwrap();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {target:?}"));
    }

    #[tokio::test]
    async fn transient_drop_recovers_to_up_keeping_the_same_local_addr() {
        // connect succeeds, one health-check failure (the "drop"), then the
        // reconnect succeeds.
        let transport = ScriptedTransport::new(vec![Ok(()), Err("dropped".into()), Ok(())]);
        let supervisor =
            ForwardSupervisor::spawn(&Handle::current(), addr(), transport, fast_options());
        let mut state = supervisor.state();

        wait_for(&mut state, ForwardState::Up).await;
        wait_for(&mut state, ForwardState::Reconnecting).await;
        wait_for(&mut state, ForwardState::Up).await;

        assert_eq!(supervisor.local_addr(), addr());
    }

    #[tokio::test]
    async fn sustained_failure_stays_in_reconnecting() {
        let transport = ScriptedTransport::new(vec![
            Ok(()),
            Err("dropped".into()),
            Err("still down".into()),
            Err("still down".into()),
        ]);
        let supervisor =
            ForwardSupervisor::spawn(&Handle::current(), addr(), transport, fast_options());
        let mut state = supervisor.state();

        wait_for(&mut state, ForwardState::Up).await;
        wait_for(&mut state, ForwardState::Reconnecting).await;

        // Give the sustained-failure retries time to run; state must never
        // regress to Disconnected and must not fail the test by ending.
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(*state.borrow(), ForwardState::Reconnecting);
    }
}
