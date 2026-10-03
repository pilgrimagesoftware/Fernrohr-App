//! `pod-events-time-window` 2.1: the Events tab's time window, as a pure filter
//! over the watched rows - what it shows, what it hides, and an event ageing out.

use crate::config::ui::PodEventsWindow;
use crate::k8s::resource::events_browser::EventRow;
use crate::k8s::resource::pod_detail::events_tab::{empty_text, hidden_text};
use crate::k8s::resource::pod_detail::live_events::within_window;
use jiff::{SignedDuration, Timestamp};
use k8s_openapi::api::core::v1::Event as K8sEvent;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

fn now() -> Timestamp {
    "2026-10-02T12:00:00Z".parse().unwrap()
}

/// An event last seen `minutes_ago` before [`now`], or undated for `None`.
fn row(uid: &str, minutes_ago: Option<i64>) -> EventRow {
    EventRow::new(&K8sEvent {
        metadata: ObjectMeta {
            uid: Some(uid.into()),
            ..Default::default()
        },
        reason: Some(uid.into()),
        last_timestamp: minutes_ago.map(|minutes| Time(now() - SignedDuration::from_mins(minutes))),
        ..Default::default()
    })
}

fn uids(rows: &[EventRow]) -> Vec<&str> {
    rows.iter().map(|row| row.uid.as_str()).collect()
}

#[test]
fn each_window_shows_what_was_seen_within_it_and_counts_the_rest() {
    let rows = [
        row("5m", Some(5)),
        row("30m", Some(30)),
        row("3h", Some(180)),
        row("2d", Some(48 * 60)),
    ];
    let cases: [(PodEventsWindow, &[&str], usize); 5] = [
        (PodEventsWindow::Minutes15, &["5m"], 3),
        (PodEventsWindow::Hour1, &["5m", "30m"], 2),
        (PodEventsWindow::Hours6, &["5m", "30m", "3h"], 1),
        (PodEventsWindow::Hours24, &["5m", "30m", "3h"], 1),
        (PodEventsWindow::All, &["5m", "30m", "3h", "2d"], 0),
    ];
    for (window, shown, hidden) in cases {
        let (kept, left_out) = within_window(&rows, window, now());
        assert_eq!(uids(&kept), shown, "{window:?}");
        assert_eq!(left_out, hidden, "{window:?}");
    }
}

#[test]
fn an_undated_event_is_kept_since_its_age_cant_be_told() {
    let (kept, hidden) = within_window(&[row("undated", None)], PodEventsWindow::Minutes15, now());
    assert_eq!(uids(&kept), ["undated"]);
    assert_eq!(hidden, 0);
}

/// The same event, re-checked later as the minute tick does, leaves the window.
#[test]
fn an_event_ages_out_of_the_window() {
    let rows = [row("recent", Some(50))];
    let (kept, _) = within_window(&rows, PodEventsWindow::Hour1, now());
    assert_eq!(uids(&kept), ["recent"]);
    let later = now() + SignedDuration::from_mins(15);
    let (kept, hidden) = within_window(&rows, PodEventsWindow::Hour1, later);
    assert!(kept.is_empty());
    assert_eq!(hidden, 1);
}

#[test]
fn the_wording_names_the_window_and_the_hidden_count() {
    assert_eq!(
        empty_text(PodEventsWindow::Hour1),
        "No events in the last 1 hour."
    );
    assert_eq!(empty_text(PodEventsWindow::All), "No events.");
    assert_eq!(hidden_text(1), "1 older event hidden");
    assert_eq!(hidden_text(3), "3 older events hidden");
}
