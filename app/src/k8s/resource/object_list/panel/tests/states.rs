//! `list-loading-indicator` 2.1-2.2 in a generic list panel: the delayed
//! first-load indicator with its running count, the empty states, and a
//! relist that keeps its rows with a refreshing spinner in the header. The
//! panel's own store is fed watch events as a watch would.

use super::{Harness, deployments, focus_table, harness, nodes, object, press, row_names};
use crate::consts::LIST_INDICATOR_DELAY;
use crate::k8s::resource::object_list::ObjectsTable;
use crate::ui::list_state::{REFRESHING_SELECTOR, text_selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use kube::api::DynamicObject;
use kube_runtime::watcher;

/// Puts `h`'s panel on a store that hasn't listed yet.
fn unlisted(h: &mut Harness) {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.objects = cx.new(|_| ObjectsTable::default());
            cx.notify();
        })
    });
    h.vcx.run_until_parked();
}

/// Applies `events` to the panel's store, as its watch would.
fn feed(h: &mut Harness, events: Vec<watcher::Event<DynamicObject>>) {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let objects = panel.read(cx).objects.clone();
        objects.update(cx, |table, _| {
            for event in events {
                table.apply(event);
            }
        });
        panel.update(cx, |_, cx| cx.notify());
    });
    h.vcx.run_until_parked();
}

fn wait_out_the_delay(h: &mut Harness) {
    h.vcx.executor().advance_clock(LIST_INDICATOR_DELAY);
    h.vcx.run_until_parked();
}

fn drawn(h: &mut Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

fn says(h: &mut Harness, text: &str) -> bool {
    drawn(h, text_selector(text))
}

#[gpui_kit::test]
async fn a_slow_first_list_shows_loading_after_the_delay(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), Vec::new());
    unlisted(&mut h);
    feed(
        &mut h,
        vec![
            watcher::Event::Init,
            watcher::Event::InitApply(object("web", Some("team-a"))),
        ],
    );
    assert!(
        !says(&mut h, "Loading Deployments…"),
        "not before the delay"
    );

    wait_out_the_delay(&mut h);
    assert!(says(&mut h, "Loading Deployments…"));
    assert!(says(&mut h, "1 received"));

    feed(&mut h, vec![watcher::Event::InitDone]);
    assert!(!says(&mut h, "Loading Deployments…"));
    assert_eq!(row_names(&mut h), ["web"]);
}

#[gpui_kit::test]
async fn a_fast_first_list_never_shows_loading(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), Vec::new());
    unlisted(&mut h);
    feed(&mut h, vec![watcher::Event::Init]);
    feed(
        &mut h,
        vec![
            watcher::Event::InitApply(object("web", Some("team-a"))),
            watcher::Event::InitDone,
        ],
    );
    wait_out_the_delay(&mut h);
    assert!(!says(&mut h, "Loading Deployments…"));
    assert_eq!(row_names(&mut h), ["web"]);
}

#[gpui_kit::test]
async fn an_empty_namespaced_list_names_its_scope(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), Vec::new());
    assert!(says(&mut h, "No Deployments in any namespace"));
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.set_namespaces(vec!["team-a".into()], cx)
        })
    });
    h.vcx.run_until_parked();
    assert!(says(&mut h, "No Deployments in team-a"));
}

#[gpui_kit::test]
async fn an_empty_cluster_scoped_list_names_only_its_kind(cx: &mut TestAppContext) {
    let mut h = harness(cx, nodes(), Vec::new());
    assert!(says(&mut h, "No Nodes"));
}

#[gpui_kit::test]
async fn a_filter_that_hides_every_row_says_so(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("team-a"))]);
    focus_table(&mut h);
    press(&mut h.vcx, "/ z z z");
    assert!(says(&mut h, "No rows match the filter"));
}

#[gpui_kit::test]
async fn a_relist_keeps_the_rows_and_shows_it_is_refreshing(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        deployments(),
        vec![object("web", Some("team-a")), object("api", Some("team-a"))],
    );
    feed(&mut h, vec![watcher::Event::Init]);
    assert_eq!(row_names(&mut h), ["api", "web"], "the rows stay");
    assert!(
        !drawn(&mut h, REFRESHING_SELECTOR.into()),
        "not before the delay"
    );
    wait_out_the_delay(&mut h);
    assert!(drawn(&mut h, REFRESHING_SELECTOR.into()));

    feed(
        &mut h,
        vec![
            watcher::Event::InitApply(object("db", Some("team-a"))),
            watcher::Event::InitDone,
        ],
    );
    assert_eq!(row_names(&mut h), ["db"], "the new list replaced them");
    assert!(!drawn(&mut h, REFRESHING_SELECTOR.into()));
}
