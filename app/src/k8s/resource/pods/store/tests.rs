// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::k8s::resource::pods::PodsTable;
use kube_runtime::watcher;

use crate::k8s::resource::pods::test_support::*;

#[test]
fn initial_stream_populates_rows() {
    let mut table = PodsTable::new();

    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
    table.apply(watcher::Event::InitApply(pod("u2", "web-2")));
    table.apply(watcher::Event::InitDone);

    assert_eq!(table.pods().len(), 2);
}

#[test]
fn later_apply_and_delete_update_rows_within_the_drain_cycle() {
    let mut table = PodsTable::new();
    table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
    table.apply(watcher::Event::InitApply(pod("u2", "web-2")));

    table.apply(watcher::Event::Apply(pod("u3", "web-3")));
    assert_eq!(table.pods().len(), 3);

    table.apply(watcher::Event::Delete(pod("u1", "web-1")));
    assert_eq!(table.pods().len(), 2);
    assert!(
        table
            .pods()
            .iter()
            .all(|p| p.metadata.uid.as_deref() != Some("u1"))
    );
}

#[test]
fn reconnect_converges_to_the_post_interruption_state() {
    let mut table = PodsTable::new();

    // Initial connection: three pods.
    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
    table.apply(watcher::Event::InitApply(pod("u2", "web-2")));
    table.apply(watcher::Event::InitApply(pod("u3", "web-3")));
    table.apply(watcher::Event::InitDone);
    assert_eq!(table.pods().len(), 3);

    // Stream interrupted and reconnects: web-1 was deleted while
    // disconnected (no explicit Delete event ever arrives for it), and
    // web-4 appeared. The relist only re-sends what's there now.
    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(pod("u2", "web-2")));
    table.apply(watcher::Event::InitApply(pod("u3", "web-3")));
    table.apply(watcher::Event::InitApply(pod("u4", "web-4")));
    table.apply(watcher::Event::InitDone);

    let mut names: Vec<_> = table
        .pods()
        .iter()
        .map(|p| p.metadata.name.clone().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, vec!["web-2", "web-3", "web-4"]);
}

/// `list-loading-indicator` 1.1: the Pods table's phase follows its lists.
#[test]
fn the_phase_follows_a_first_list_and_a_relist() {
    use crate::k8s::resource::load_phase::LoadPhase;
    let mut table = PodsTable::new();
    table.apply(watcher::Event::Init);
    table.apply(watcher::Event::InitApply(pod("u1", "web")));
    table.apply(watcher::Event::InitApply(pod("u2", "api")));
    assert_eq!(table.phase(), LoadPhase::FirstLoad { received: 2 });
    table.apply(watcher::Event::InitDone);
    assert_eq!(table.phase(), LoadPhase::Loaded);
    table.apply(watcher::Event::Init);
    assert_eq!(table.phase(), LoadPhase::Refreshing { received: 0 });
    assert_eq!(table.pods().len(), 2, "a refresh keeps the rows");
}
