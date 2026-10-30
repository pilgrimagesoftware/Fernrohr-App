//! #275 in a list panel: rows a live watch event adds, changes or deletes flash
//! in their change's tone - but not for the first list, a relist, or a change
//! nothing drawn shows - and a flash ends, a deleted row with it.

use super::{Harness, deployments, harness, object};
use crate::consts::ROW_FLASH;
use crate::k8s::resource::object_list::table::ObjectTableDelegate;
use crate::ui::row_flash::RowChange;
use gpui_kit::test::TestWindowExt as _;
use kube::api::DynamicObject;
use kube_runtime::watcher;
use serde_json::json;

/// Applies `events` to the panel's store, as its watch would, and draws.
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
    h.vcx.update(|window, cx| window.render_frame(cx));
}

fn flash(h: &mut Harness, name: &str) -> Option<RowChange> {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone().expect("the table is drawn");
        table.read(cx).delegate().flash_of(&format!("uid-{name}"))
    })
}

fn names(h: &mut Harness) -> Vec<String> {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone().expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .map(|row| row.object.name.clone())
            .collect()
    })
}

/// `name`, ready `ready` of 2, at `version`.
fn deployment(name: &str, ready: i64, version: &str) -> DynamicObject {
    let mut object = object(name, Some("team-a"));
    object.metadata.resource_version = Some(version.into());
    object.data =
        json!({ "spec": { "replicas": 2 }, "status": { "readyReplicas": ready, "replicas": 2 } });
    object
}

fn listed(cx: &mut gpui_kit::TestAppContext) -> Harness {
    let mut h = harness(
        cx,
        deployments(),
        vec![deployment("web", 1, "1"), deployment("api", 1, "1")],
    );
    h.vcx.update(|window, cx| window.render_frame(cx));
    h
}

#[gpui_kit::test]
async fn the_first_list_flashes_nothing(cx: &mut gpui_kit::TestAppContext) {
    let mut h = listed(cx);
    assert_eq!(flash(&mut h, "web"), None);
    assert_eq!(flash(&mut h, "api"), None);
}

#[gpui_kit::test]
async fn added_changed_and_deleted_rows_flash_their_tone(cx: &mut gpui_kit::TestAppContext) {
    let mut h = listed(cx);
    feed(
        &mut h,
        vec![
            watcher::Event::Apply(deployment("worker", 0, "1")),
            watcher::Event::Apply(deployment("web", 2, "2")),
            watcher::Event::Delete(deployment("api", 1, "1")),
        ],
    );
    assert_eq!(flash(&mut h, "worker"), Some(RowChange::Added));
    assert_eq!(flash(&mut h, "web"), Some(RowChange::Modified));
    assert_eq!(flash(&mut h, "api"), Some(RowChange::Deleted));
    assert!(
        names(&mut h).contains(&"api".to_string()),
        "the deleted row stays for its flash"
    );
}

/// A `resourceVersion` bump that changes no drawn cell doesn't flash.
#[gpui_kit::test]
async fn a_change_nothing_shows_does_not_flash(cx: &mut gpui_kit::TestAppContext) {
    let mut h = listed(cx);
    feed(
        &mut h,
        vec![watcher::Event::Apply(deployment("web", 1, "2"))],
    );
    assert_eq!(flash(&mut h, "web"), None);
}

/// A relist - a reconnect's or a poll's - replays every object: none flashes,
/// not even one it changes or removes.
#[gpui_kit::test]
async fn a_relist_flashes_nothing(cx: &mut gpui_kit::TestAppContext) {
    let mut h = listed(cx);
    feed(
        &mut h,
        vec![
            watcher::Event::Init,
            watcher::Event::InitApply(deployment("web", 2, "2")),
            watcher::Event::InitApply(deployment("worker", 0, "1")),
            watcher::Event::InitDone,
        ],
    );
    for name in ["web", "worker", "api"] {
        assert_eq!(flash(&mut h, name), None, "{name}");
    }
    assert!(
        !names(&mut h).contains(&"api".to_string()),
        "swept, not flashed"
    );
}

/// A flash ends after its time, and a deleted row goes with it.
#[gpui_kit::test]
async fn a_flash_ends_and_a_deleted_row_goes(cx: &mut gpui_kit::TestAppContext) {
    let mut h = listed(cx);
    feed(
        &mut h,
        vec![
            watcher::Event::Apply(deployment("web", 2, "2")),
            watcher::Event::Delete(deployment("api", 1, "1")),
        ],
    );
    assert_eq!(flash(&mut h, "web"), Some(RowChange::Modified));

    h.vcx.executor().advance_clock(ROW_FLASH);
    h.vcx.run_until_parked();
    h.vcx.update(|window, cx| window.render_frame(cx));
    assert_eq!(flash(&mut h, "web"), None, "the flash ended");
    assert_eq!(flash(&mut h, "api"), None);
    assert_eq!(names(&mut h), ["web"], "and the deleted row is gone");
}

/// The bulk selection's object names, from the panel the bar reads it from.
fn checked(h: &mut Harness) -> Vec<String> {
    use crate::ui::bulk::BulkHost as _;
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        panel
            .read(cx)
            .bulk_selection(cx)
            .map(|selection| {
                selection
                    .targets
                    .into_iter()
                    .map(|target| target.name)
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Review fix (1): a deleted row still drawn for its flash is never checked -
/// one that was drops out, and select-all passes it by.
#[gpui_kit::test]
async fn a_deleted_rows_ghost_is_never_checked(cx: &mut gpui_kit::TestAppContext) {
    let mut h = listed(cx);
    // The delegate's own checks - what `x` and select-all run, whose keys this
    // harness doesn't bind (their keystrokes are covered in `bulk`'s tests).
    fn edit(h: &mut Harness, check: fn(&mut ObjectTableDelegate)) {
        let panel = h.panel.clone();
        h.vcx.update(|_, cx| {
            let table = panel.read(cx).table.clone().expect("drawn");
            table.update(cx, |table, cx| {
                check(table.delegate_mut());
                cx.notify();
            });
        });
        h.vcx.run_until_parked();
    }
    edit(&mut h, |delegate| delegate.check_row(0, false));
    let first = checked(&mut h).remove(0);
    let (gone, kept) = if first == "api" {
        ("api", "web")
    } else {
        ("web", "api")
    };

    feed(
        &mut h,
        vec![watcher::Event::Delete(deployment(gone, 1, "1"))],
    );
    assert!(
        names(&mut h).contains(&gone.to_string()),
        "still drawn, flashing"
    );
    assert!(checked(&mut h).is_empty(), "the checked row dropped out");

    edit(&mut h, |delegate| delegate.check_all());
    assert_eq!(
        checked(&mut h),
        [kept],
        "select-all checks only the live row"
    );
}

/// Review fix (2): the cursor stays on a row as it is deleted, but nothing acts
/// on its ghost - no selection to act on, and Enter opens nothing.
#[gpui_kit::test]
async fn no_action_reaches_a_ghost_under_the_cursor(cx: &mut gpui_kit::TestAppContext) {
    let opened = super::record_opens(cx);
    let mut h = listed(cx);
    super::focus_table(&mut h);
    super::press(&mut h.vcx, "down");
    let panel = h.panel.clone();
    let cursor_name = h.vcx.update(|_, cx| {
        let panel = panel.read(cx);
        let row_ix = panel.selected_row(cx).expect("a row is selected");
        let table = panel.table.clone().expect("drawn");
        table.read(cx).delegate().rows()[row_ix].object.name.clone()
    });

    feed(
        &mut h,
        vec![watcher::Event::Delete(deployment(&cursor_name, 1, "1"))],
    );
    let (cursor, acted_on) = h.vcx.update(|_, cx| {
        let panel = panel.read(cx);
        let table = panel.table.clone().expect("drawn");
        (table.read(cx).selected_row(), panel.selected_row(cx))
    });
    assert!(cursor.is_some(), "the cursor stays on the ghost");
    assert_eq!(acted_on, None, "but no action sees a selection there");

    super::press(&mut h.vcx, "enter");
    assert!(opened.borrow().is_empty(), "Enter opens nothing");
}

/// Review fix (3): emptiness is judged without ghosts - a filter matching only
/// a deleted row reads "No rows match" once its flash ends, and a list whose
/// last row is deleted ends empty.
#[gpui_kit::test]
async fn emptiness_ignores_ghosts(cx: &mut gpui_kit::TestAppContext) {
    use crate::ui::list_state::{EMPTY_SELECTOR, FILTERED_EMPTY, text_selector};
    let mut h = listed(cx);
    super::focus_table(&mut h);
    super::press(&mut h.vcx, "/");
    h.vcx.simulate_input("api");
    h.vcx.run_until_parked();
    feed(
        &mut h,
        vec![watcher::Event::Delete(deployment("api", 1, "1"))],
    );
    assert_eq!(names(&mut h), ["api"], "only the ghost is drawn");

    h.vcx.executor().advance_clock(ROW_FLASH);
    h.vcx.run_until_parked();
    let filtered: &'static str = text_selector(FILTERED_EMPTY).leak();
    h.vcx.update(|window, cx| window.render_frame(cx));
    assert!(h.vcx.debug_bounds(EMPTY_SELECTOR).is_some());
    assert!(
        h.vcx.debug_bounds(filtered).is_some(),
        "No rows match the filter"
    );
}

#[gpui_kit::test]
async fn a_list_whose_last_row_is_deleted_ends_empty(cx: &mut gpui_kit::TestAppContext) {
    use crate::ui::list_state::text_selector;
    let mut h = harness(cx, deployments(), vec![deployment("web", 1, "1")]);
    h.vcx.update(|window, cx| window.render_frame(cx));
    feed(
        &mut h,
        vec![watcher::Event::Delete(deployment("web", 1, "1"))],
    );

    h.vcx.executor().advance_clock(ROW_FLASH);
    h.vcx.run_until_parked();
    h.vcx.update(|window, cx| window.render_frame(cx));
    assert!(names(&mut h).is_empty(), "the ghost is gone");
    let empty: &'static str = text_selector("No Deployments in any namespace").leak();
    assert!(h.vcx.debug_bounds(empty).is_some(), "the empty state shows");
}
