//! Saving and restoring a Pods panel's active sort with the window's dock
//! layout (`saved-panel-layouts` 1.6). `dump`'s two branches - the table
//! built, or not yet drawn - are each exercised, plus the restore side's
//! column-id lookup.

// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for the plain synchronous test below.
use super::sort_from_state;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::resource::pods::PodsPanel;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::dock::{BasePanel as _, PanelInfo};
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};

fn test_client(cx: &mut TestAppContext) -> kube::Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

/// A panel never drawn saves the sort it was given to start with (`initial_sort`),
/// rather than losing it for having no live table yet to read from.
#[gpui_kit::test]
async fn an_undrawn_panels_dump_falls_back_to_its_initial_sort(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let panel = cx.update(|cx| {
        cx.new(|cx| {
            let mut panel =
                PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "dev".into()), cx);
            panel.initial_sort = Some(("age".to_string(), true));
            panel
        })
    });
    let state = cx.update(|cx| panel.read(cx).dump(cx));
    let PanelInfo::Panel(data) = state.info else {
        panic!("a Pods panel saves panel state");
    };
    let sort = sort_from_state(&data).expect("the saved sort reads back");
    assert_eq!(sort, ("age".to_string(), true));
}

/// Once the table is built, a changed sort is what the panel saves - read
/// from the live table, not the (by-then-stale) `initial_sort` it started
/// with.
#[gpui_kit::test]
async fn a_changed_sorts_dump_reads_the_live_table(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let client = test_client(cx);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::pods(), "dev".into());
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
        let namespaces = cx.new(|_| NamespaceList::empty());
        let panel = cx.new(|cx| PodsPanel::with_connection(scope, connection, namespaces, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    vcx.update(|_, cx| {
        let table = panel
            .read(cx)
            .pod_table
            .clone()
            .expect("the table is drawn by now");
        table.update(cx, |table, _| {
            table.delegate_mut().set_sort_state("status", true)
        });
    });

    let state = vcx.update(|_, cx| panel.read(cx).dump(cx));
    let PanelInfo::Panel(data) = state.info else {
        panic!("a Pods panel saves panel state");
    };
    let sort = sort_from_state(&data).expect("the saved sort reads back");
    assert_eq!(sort, ("status".to_string(), true));
}

/// A saved sort naming a column this build doesn't have is ignored on
/// restore: `sort_from_state` still reads it back as asked (restore itself
/// doesn't invent a fallback column), but applying it leaves the table
/// unsorted - covered at the delegate level by
/// `pods_table::tests::saved_sort::an_unknown_saved_column_is_ignored`.
#[test]
fn sort_from_state_reads_null_as_no_saved_sort() {
    assert_eq!(sort_from_state(&serde_json::json!({})), None);
    assert_eq!(sort_from_state(&serde_json::json!({ "sort": null })), None);
}
