//! `connection-status-bar`'s health/severity model: what one cluster context's status
//! bar item shows and what color it gets. Both are pure - no GPUI dependency - so the
//! spec's precedence and color table are unit-testable without a `TestAppContext`.
//! [`super::session::ClusterRegistry::health`] is the GPUI-facing producer of
//! [`ContextHealth`]; `ui/status_bar.rs` is the consumer of both types here.

use super::watch_registry::PauseReason;
use crate::consts::STATUS_ESCALATE_AFTER;
use std::time::Instant;

/// One cluster context's connection health, combining `ConnectionState` and its watch
/// registry's first paused key (design.md decision 1) into the single signal the status
/// bar renders.
///
/// `ConnectionState::Connecting` has no variant of its own here: the spec's color table
/// has no distinct "connecting" row, and a window's status bar only ever shows a context
/// after the cluster picker has already taken it past that state (see proposal.md's
/// non-goals) - so an in-flight first connect reads the same as a healthy one.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextHealth {
    /// Nothing to report: connecting, or connected, with no watch paused.
    Connected,
    WaitingForTunnel {
        since: Instant,
    },
    Paused {
        reason: PauseReason,
        since: Instant,
    },
    Failed {
        reason: String,
        since: Instant,
    },
    /// Waiting for the user to confirm its manual tunnel (`manual-confirmation-tunnels`
    /// D8): the network path the tunnel stands for is the user's to bring up.
    AwaitingConfirmation {
        /// The tunnel's id, for Proceed and Cancel to resolve.
        tunnel_id: String,
        /// The tunnel's display name.
        tunnel: String,
        /// The tunnel's instruction, if it has one.
        message: Option<String>,
        since: Instant,
    },
}

/// The status bar's color for one item, from the spec's color table - a value, not a
/// theme color directly, so this module stays free of any GPUI/theme dependency.
/// `ui/status_bar.rs` maps each variant to `cx.theme()`'s matching semantic color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Muted,
    Info,
    Warning,
    Danger,
    /// Needs the user to act - a manual tunnel awaiting confirmation. The one
    /// severity drawn as a fill rather than a text tint, so it stands out from
    /// everything else in the row.
    Attention,
}

/// Maps `health` to its color at `now`, per the spec's table: connected is muted;
/// waiting for a tunnel or a credential refresh is info; a tunnel reconnecting is
/// warning; a failed connection, or any pause lasting longer than
/// [`STATUS_ESCALATE_AFTER`], is danger; and a manual tunnel awaiting the user's
/// confirmation is attention, however long it has waited - it has no timeout.
pub fn severity(health: &ContextHealth, now: Instant) -> Severity {
    match health {
        ContextHealth::Connected => Severity::Muted,
        ContextHealth::WaitingForTunnel { .. } => Severity::Info,
        ContextHealth::Failed { .. } => Severity::Danger,
        ContextHealth::AwaitingConfirmation { .. } => Severity::Attention,
        ContextHealth::Paused { reason, since } => {
            if now.saturating_duration_since(*since) > STATUS_ESCALATE_AFTER {
                Severity::Danger
            } else {
                match reason {
                    PauseReason::Reconnecting => Severity::Warning,
                    PauseReason::CredentialRefresh => Severity::Info,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// One row per line of the spec's color table (`specs/app-shell/spec.md`'s "Status
    /// severity is visually distinct"), plus the 29 s/31 s escalation boundary named in
    /// tasks.md 1.2.
    #[test]
    fn severity_matches_the_spec_color_table() {
        let now = Instant::now();

        assert_eq!(severity(&ContextHealth::Connected, now), Severity::Muted);
        assert_eq!(
            severity(&ContextHealth::WaitingForTunnel { since: now }, now),
            Severity::Info
        );
        assert_eq!(
            severity(
                &ContextHealth::Paused {
                    reason: PauseReason::CredentialRefresh,
                    since: now
                },
                now
            ),
            Severity::Info
        );
        assert_eq!(
            severity(
                &ContextHealth::Paused {
                    reason: PauseReason::Reconnecting,
                    since: now
                },
                now
            ),
            Severity::Warning
        );
        assert_eq!(
            severity(
                &ContextHealth::Failed {
                    reason: "boom".to_string(),
                    since: now
                },
                now
            ),
            Severity::Danger
        );
    }

    #[test]
    fn a_reconnecting_pause_at_29_seconds_is_still_warning() {
        let now = Instant::now();
        let since = now - Duration::from_secs(29);
        assert_eq!(
            severity(
                &ContextHealth::Paused {
                    reason: PauseReason::Reconnecting,
                    since
                },
                now
            ),
            Severity::Warning
        );
    }

    #[test]
    fn a_reconnecting_pause_at_31_seconds_escalates_to_danger() {
        let now = Instant::now();
        let since = now - Duration::from_secs(31);
        assert_eq!(
            severity(
                &ContextHealth::Paused {
                    reason: PauseReason::Reconnecting,
                    since
                },
                now
            ),
            Severity::Danger
        );
    }

    /// The 30 s escalation isn't reconnecting-only: the spec's danger row is "any pause",
    /// so a long credential-refresh pause escalates too even though its normal color is
    /// info rather than warning.
    #[test]
    fn a_credential_refresh_pause_also_escalates_past_30_seconds() {
        let now = Instant::now();
        let since = now - Duration::from_secs(31);
        assert_eq!(
            severity(
                &ContextHealth::Paused {
                    reason: PauseReason::CredentialRefresh,
                    since
                },
                now
            ),
            Severity::Danger
        );
    }

    /// `manual-confirmation-tunnels` 4.1: awaiting confirmation is attention, and
    /// stays attention however long it waits - it has no timeout to escalate past.
    #[test]
    fn awaiting_confirmation_is_attention_at_any_age() {
        let since = Instant::now();
        let health = ContextHealth::AwaitingConfirmation {
            tunnel_id: "t".into(),
            tunnel: "corp-vpn".into(),
            message: None,
            since,
        };
        assert_eq!(severity(&health, since), Severity::Attention);
        let later = since + STATUS_ESCALATE_AFTER * 10;
        assert_eq!(severity(&health, later), Severity::Attention);
    }
}
