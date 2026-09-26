use crate::managed_forward::ForwardState;
use tokio::sync::{mpsc, watch};

/// What a forward state transition means for the cluster's watchers - `None` for any
/// transition that isn't a health edge (e.g. Connecting -> Up on first connect, which
/// `ClusterConnection`'s own `WaitingForTunnel` already covers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthTransition {
    Pause,
    Resume,
}

/// The pure decision core: Up -> Reconnecting pauses, Reconnecting -> Up resumes.
/// Everything else (including the very first `Up`, whose `prev` is `Connecting`) is not
/// a health edge - there is nothing to recover from yet.
fn transition(prev: ForwardState, next: ForwardState) -> Option<HealthTransition> {
    match (prev, next) {
        (ForwardState::Up, ForwardState::Reconnecting) => Some(HealthTransition::Pause),
        (ForwardState::Reconnecting, ForwardState::Up) => Some(HealthTransition::Resume),
        _ => None,
    }
}

/// Watches a bound context's forward state and reports each pause/resume edge on `tx`,
/// factored out of the GPUI wiring in [`super::session::ClusterSession`] so it's testable
/// against a plain `watch::Sender` with no app context - the same seam `connect_and_probe`
/// (section 6.2) already established one layer over. Runs until `state_rx`'s sender drops.
pub async fn drive(
    mut state_rx: watch::Receiver<ForwardState>,
    tx: mpsc::Sender<HealthTransition>,
) {
    let mut prev = *state_rx.borrow();
    while state_rx.changed().await.is_ok() {
        let next = *state_rx.borrow();
        if let Some(edge) = transition(prev, next)
            && tx.send(edge).await.is_err()
        {
            return;
        }
        prev = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_to_reconnecting_pauses() {
        assert_eq!(
            transition(ForwardState::Up, ForwardState::Reconnecting),
            Some(HealthTransition::Pause)
        );
    }

    #[test]
    fn reconnecting_to_up_resumes() {
        assert_eq!(
            transition(ForwardState::Reconnecting, ForwardState::Up),
            Some(HealthTransition::Resume)
        );
    }

    #[test]
    fn first_connect_is_not_a_health_edge() {
        assert_eq!(transition(ForwardState::Connecting, ForwardState::Up), None);
    }

    #[test]
    fn disconnected_edges_are_not_health_edges() {
        assert_eq!(
            transition(ForwardState::Disconnected, ForwardState::Connecting),
            None
        );
    }

    #[tokio::test]
    async fn drive_reports_pause_then_resume_in_order() {
        let (state_tx, state_rx) = watch::channel(ForwardState::Up);
        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(drive(state_rx, tx));
        // Without this, `state_tx.send` below can land before `drive`'s first poll
        // borrows the initial value, so its `prev` already reads the sent value and
        // the edge is invisible - the same fixture race noted for `ForwardSupervisor`'s
        // fakes (section 2.3): a real transport always has a genuine await point
        // between spawn and first state change, but nothing here forces one.
        tokio::task::yield_now().await;

        state_tx.send(ForwardState::Reconnecting).unwrap();
        assert_eq!(rx.recv().await, Some(HealthTransition::Pause));

        state_tx.send(ForwardState::Up).unwrap();
        assert_eq!(rx.recv().await, Some(HealthTransition::Resume));

        drop(state_tx);
        handle.await.unwrap();
        assert_eq!(rx.recv().await, None);
    }

    #[tokio::test]
    async fn drive_stops_when_the_receiver_side_drops() {
        let (state_tx, state_rx) = watch::channel(ForwardState::Up);
        let (tx, rx) = mpsc::channel(4);
        drop(rx);

        // Spawn (rather than await directly under the timeout) and yield once so
        // `drive` borrows its initial value before the send below - see the comment
        // on the same pattern in `drive_reports_pause_then_resume_in_order`.
        let handle = tokio::spawn(drive(state_rx, tx));
        tokio::task::yield_now().await;
        state_tx.send(ForwardState::Reconnecting).unwrap();

        // Must return promptly once `tx.send` fails, rather than looping forever.
        tokio::time::timeout(std::time::Duration::from_millis(200), handle)
            .await
            .expect("drive must return once the receiver is gone")
            .unwrap();
    }
}
