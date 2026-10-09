//! Section 2.3 of the tunnel-subsystem change: the per-forward supervisor task that
//! drives a `ManagedForward` implementation's [`ForwardState`](crate::forward::managed::ForwardState)
//! machine - connect, periodic health check, and reconnect-with-backoff on failure.
//!
//! `SshTunnel` and `K8sPortForward` (sections 3-4) each supply a [`ForwardTransport`]
//! (spawn/supervise an `ssh` child, or hold a `kube` port-forward stream) and let this
//! module own the retry policy so neither has to reimplement it.
//!
//! A transport can also give up for good (`manual-confirmation-tunnels` D3: the user
//! cancelled a manual tunnel): [`ConnectFailure::GiveUp`] ends the supervisor, which
//! closes the forward's state channel - the only sender is the task's - and leaves the
//! reason in [`ForwardSupervisor::failure`] for whoever was waiting to report.

use crate::forward::managed::ForwardState;
use parking_lot::Mutex;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
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

    /// [`Self::connect`], saying whether a failure may be retried. The supervisor calls
    /// this; by default every failure is retried with backoff, so only a transport with
    /// a failure that must not be retried - a user's Cancel - overrides it.
    fn connect_outcome(&mut self) -> impl Future<Output = Result<(), ConnectFailure>> + Send {
        async move { self.connect().await.map_err(ConnectFailure::Retry) }
    }
}

/// Why a connect failed, and what the supervisor does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectFailure {
    /// Try again after the backoff: the bastion may come back.
    Retry(String),
    /// Stop for good: the forward closes, failing everything waiting on it with this
    /// reason.
    GiveUp(String),
}

/// Where a supervisor that gave up leaves its reason, shared with whoever waits on
/// its state channel so they can report it once the channel closes.
#[derive(Clone, Debug, Default)]
pub struct FailureSlot(Arc<Mutex<Option<String>>>);

impl FailureSlot {
    /// The reason the supervisor gave up, if it did.
    pub fn get(&self) -> Option<String> {
        self.0.lock().clone()
    }

    fn set(&self, reason: String) {
        *self.0.lock() = Some(reason);
    }

    /// Fills the slot as a supervisor that gave up would. Test-only.
    #[cfg(test)]
    pub fn set_for_test(&self, reason: &str) {
        self.set(reason.to_string());
    }
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
    /// A receiver to subscribe from. The task holds the only sender, so the channel
    /// closes when the task ends - which it does only after giving up.
    state_rx: watch::Receiver<ForwardState>,
    failure: FailureSlot,
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
        let (state_tx, state_rx) = watch::channel(ForwardState::Disconnected);
        let failure = FailureSlot::default();
        let task = rt.spawn(run(state_tx, transport, options, failure.clone()));
        Self {
            state_rx,
            failure,
            local_addr,
            task,
        }
    }

    #[allow(dead_code)]
    pub fn state(&self) -> watch::Receiver<ForwardState> {
        self.state_rx.clone()
    }

    /// Where the reason goes if the transport gives up.
    pub fn failure(&self) -> FailureSlot {
        self.failure.clone()
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
    failure: FailureSlot,
) {
    let mut has_been_up = false;
    let mut attempt: u32 = 0;
    loop {
        state_tx.send_replace(if has_been_up {
            ForwardState::Reconnecting
        } else {
            ForwardState::Connecting
        });

        match transport.connect_outcome().await {
            Ok(()) => {}
            Err(ConnectFailure::Retry(_reason)) => {
                attempt += 1;
                tokio::time::sleep(options.backoff.delay(attempt)).await;
                continue;
            }
            Err(ConnectFailure::GiveUp(reason)) => {
                // Set before returning drops `state_tx`, so a waiter that sees the
                // channel close finds the reason already there.
                failure.set(reason);
                return;
            }
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

    /// A transport that gives up on its first connect.
    struct GivingUp;

    impl ForwardTransport for GivingUp {
        async fn connect(&mut self) -> Result<(), String> {
            unreachable!("the supervisor asks for the outcome")
        }

        async fn health_check(&mut self) -> Result<(), String> {
            Ok(())
        }

        async fn connect_outcome(&mut self) -> Result<(), ConnectFailure> {
            tokio::task::yield_now().await;
            Err(ConnectFailure::GiveUp("corp-vpn was cancelled".into()))
        }
    }

    #[tokio::test]
    async fn giving_up_closes_the_state_channel_and_leaves_the_reason() {
        let supervisor =
            ForwardSupervisor::spawn(&Handle::current(), addr(), GivingUp, fast_options());
        let mut state = supervisor.state();
        let closed = tokio::time::timeout(Duration::from_secs(2), async {
            while state.changed().await.is_ok() {}
        })
        .await;
        assert!(closed.is_ok(), "the channel closes");
        assert_eq!(
            supervisor.failure().get().as_deref(),
            Some("corp-vpn was cancelled")
        );
        assert_ne!(*state.borrow(), ForwardState::Up, "never reported Up");
    }

    #[tokio::test]
    async fn a_retryable_failure_never_closes_the_channel() {
        let transport = ScriptedTransport::new(vec![Err("down".into())]);
        let supervisor =
            ForwardSupervisor::spawn(&Handle::current(), addr(), transport, fast_options());
        let mut state = supervisor.state();
        let changed = tokio::time::timeout(Duration::from_millis(80), async {
            loop {
                if state.changed().await.is_err() {
                    return "closed";
                }
            }
        })
        .await;
        assert!(changed.is_err(), "still retrying, channel open");
        assert_eq!(supervisor.failure().get(), None);
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
