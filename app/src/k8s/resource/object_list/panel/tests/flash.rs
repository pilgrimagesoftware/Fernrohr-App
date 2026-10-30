//! #275 in a list panel: rows a live watch event adds, changes or deletes flash
//! in their change's tone - but not for the first list, a relist, or a change
//! nothing drawn shows - and a flash ends, a deleted row with it.

use super::{Harness, deployments, harness, object};
use crate::consts::ROW_FLASH;
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
