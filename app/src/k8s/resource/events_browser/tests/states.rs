//! `list-loading-indicator` in the events browser: the same delayed loading
//! indicator, empty state and header refreshing spinner as the resource
//! lists, naming events.

use super::*;
use crate::consts::LIST_INDICATOR_DELAY;
use crate::ui::list_state::{REFRESHING_SELECTOR, text_selector};
use gpui_kit::test::TestWindowExt as _;

/// Applies `events` to the browser's store, as its watch would.
fn feed(h: &mut Harness, events: Vec<watcher::Event<K8sEvent>>) {
    let (table, panel) = (h.events.clone(), h.panel.clone());
    h.vcx.update(|_, cx| {
        table.update(cx, |table, _| {
            for event in events {
                table.apply(event);
            }
        });
        panel.update(cx, |_, cx| cx.notify());
    });
    h.vcx.run_until_parked();
}

fn drawn(h: &mut Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

fn wait_out_the_delay(h: &mut Harness) {
    h.vcx.executor().advance_clock(LIST_INDICATOR_DELAY);
    h.vcx.run_until_parked();
}

#[gpui_kit::test]
async fn a_slow_first_list_of_events_shows_loading(cx: &mut TestAppContext) {
    let mut h = harness(cx, Vec::new());
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.events = cx.new(|_| EventsTable::default());
            cx.notify();
        })
    });
    h.events = h.vcx.update(|_, cx| panel.read(cx).events.clone());
    let first = fixture().remove(0);
    feed(
        &mut h,
        vec![watcher::Event::Init, watcher::Event::InitApply(first)],
    );
    assert!(!drawn(&mut h, text_selector("Loading events…")));
    wait_out_the_delay(&mut h);
    assert!(drawn(&mut h, text_selector("Loading events…")));
    assert!(drawn(&mut h, text_selector("1 received")));
}

#[gpui_kit::test]
async fn no_events_says_so(cx: &mut TestAppContext) {
    let mut h = harness(cx, Vec::new());
    assert!(drawn(&mut h, text_selector("No events in any namespace")));
}

#[gpui_kit::test]
async fn a_relist_of_events_keeps_them_and_shows_it_is_refreshing(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    let before = shown(&mut h);
    feed(&mut h, vec![watcher::Event::Init]);
    wait_out_the_delay(&mut h);
    assert_eq!(shown(&mut h), before, "the rows stay");
    assert!(drawn(&mut h, REFRESHING_SELECTOR.into()));
    feed(&mut h, vec![watcher::Event::InitDone]);
    assert!(!drawn(&mut h, REFRESHING_SELECTOR.into()));
}

/// #312: a search that hides every event keeps the table's header, with the
/// no-match row under it.
#[gpui_kit::test]
async fn a_search_that_hides_every_event_keeps_the_header(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    focus_table(&mut h);
    press(&mut h.vcx, "/");
    h.vcx.simulate_input("zzz");
    h.vcx.run_until_parked();
    crate::ui::list_state::assert_no_match_under_header(&mut h.vcx);
}

/// #275: a live event that arrives, repeats (its count goes up) or goes
/// flashes its row in its change's tone; the first list flashes nothing.
#[gpui_kit::test]
async fn live_events_flash_their_change(cx: &mut TestAppContext) {
    use crate::ui::row_flash::RowChange;
    let mut h = harness(cx, fixture());
    let flash = |h: &mut Harness, uid: &str| {
        h.vcx.update(|window, cx| {
            window.render_frame(cx);
            let table = h.panel.read(cx).table.clone().expect("the table is drawn");
            table.read(cx).delegate().flash_of(uid)
        })
    };
    assert_eq!(flash(&mut h, "e1"), None, "the first list flashes nothing");

    let mut repeated = fixture().remove(0);
    repeated.count = Some(2);
    let gone = fixture().remove(1);
    feed(
        &mut h,
        vec![
            watcher::Event::Apply(event(
                "e4",
                "Normal",
                "Pulled",
                "Pod",
                "payments",
                "web-3",
                "Pulled image",
                0,
            )),
            watcher::Event::Apply(repeated),
            watcher::Event::Delete(gone),
        ],
    );
    assert_eq!(flash(&mut h, "e4"), Some(RowChange::Added));
    assert_eq!(flash(&mut h, "e1"), Some(RowChange::Modified));
    assert_eq!(flash(&mut h, "e2"), Some(RowChange::Deleted));
    assert!(
        shown(&mut h).contains(&"e2".to_string()),
        "drawn on for its flash"
    );
}
