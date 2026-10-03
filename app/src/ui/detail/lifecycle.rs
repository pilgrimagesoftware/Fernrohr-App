//! The banner a detail panel shows above an object that is going, gone or new
//! (`live-detail-panels`): Terminating with its grace period, deleted at a time
//! with the last known state kept below as stale, or a same-name replacement.
//! Each panel decides which applies; this module owns the words and the look.

use super::BadgeTone;
use crate::k8s::resource::pods::format_age;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use jiff::Timestamp;

/// What has happened to a detail panel's object beyond its fields.
#[derive(Clone, Debug, PartialEq)]
pub enum Lifecycle {
    /// Being deleted, and gone by `deadline` - a pod's `deletionTimestamp`,
    /// which is when its grace period ends.
    Terminating { deadline: Timestamp },
    /// No longer in the cluster, as of `at`. The panel keeps the last state it
    /// saw, marked stale.
    Deleted { at: Timestamp },
    /// A new object took the name of the deleted one the panel was showing.
    Replaced,
}

impl Lifecycle {
    /// The banner's sentence about "this `noun`" at `now`.
    pub fn message(&self, noun: &str, now: Timestamp) -> String {
        match self {
            Self::Terminating { deadline } => {
                let left = deadline.duration_since(now).as_secs();
                if left > 0 {
                    format!(
                        "Terminating: this {noun}'s grace period ends in {}.",
                        format_age(left)
                    )
                } else {
                    format!(
                        "Terminating: this {noun}'s grace period ended {} ago.",
                        format_age(-left)
                    )
                }
            }
            Self::Deleted { at } => format!(
                "This {noun} was deleted at {} ({} ago). Showing its last known state.",
                at.strftime("%Y-%m-%d %H:%M:%S UTC"),
                format_age(now.duration_since(*at).as_secs())
            ),
            Self::Replaced => format!(
                "This {noun} replaced a deleted {noun} of the same name. Showing the new one."
            ),
        }
    }

    pub fn tone(&self) -> BadgeTone {
        match self {
            Self::Terminating { .. } => BadgeTone::Warning,
            Self::Deleted { .. } => BadgeTone::Bad,
            Self::Replaced => BadgeTone::Info,
        }
    }

    /// Whether what the panel shows below the banner is no longer current.
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Deleted { .. })
    }
}

/// The element id the banner carries, for tests to find it by.
pub const BANNER_ID: &str = "lifecycle-banner";

/// A detail panel's body: `content` under `lifecycle`'s banner, if there is
/// one, and drawn quieter while it shows a deleted object's last state.
pub fn body(content: AnyElement, lifecycle: Option<&Lifecycle>, noun: &str, cx: &App) -> Div {
    let stale = lifecycle.is_some_and(Lifecycle::is_stale);
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .when_some(lifecycle, |this, lifecycle| {
            this.child(banner(lifecycle, noun, Timestamp::now(), cx))
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .when(stale, |this| this.opacity(crate::consts::STALE_OPACITY))
                .child(content),
        )
}

/// The banner for `lifecycle` over "this `noun`" at `now`.
fn banner(lifecycle: &Lifecycle, noun: &str, now: Timestamp, cx: &App) -> AnyElement {
    let color = super::tone_color(lifecycle.tone(), cx);
    let space = crate::ui::space::spacing(cx);
    div()
        .id(BANNER_ID)
        .px(space.panel_inset)
        .py(space.control_gap)
        .border_b_1()
        .border_color(color)
        .text_color(color)
        .child(lifecycle.message(noun, now))
        .test_support()
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::Lifecycle;
    use jiff::{SignedDuration, Timestamp};

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(1_700_000_000 + secs).unwrap()
    }

    #[test]
    fn terminating_counts_down_the_grace_period_and_then_says_it_ended() {
        let terminating = Lifecycle::Terminating { deadline: at(30) };
        assert_eq!(
            terminating.message("pod", at(0)),
            "Terminating: this pod's grace period ends in 30s."
        );
        assert_eq!(
            terminating.message("pod", at(30) + SignedDuration::from_secs(5)),
            "Terminating: this pod's grace period ended 5s ago."
        );
        assert!(!terminating.is_stale());
    }

    #[test]
    fn deleted_says_when_and_marks_the_state_stale() {
        let deleted = Lifecycle::Deleted { at: at(0) };
        assert_eq!(
            deleted.message("deployment", at(120)),
            "This deployment was deleted at 2023-11-14 22:13:20 UTC (2m ago). Showing its last known state."
        );
        assert!(deleted.is_stale());
        assert!(!Lifecycle::Replaced.is_stale());
    }
}
