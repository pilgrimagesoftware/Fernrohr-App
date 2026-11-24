//! #177, where it was reported: a Pods panel whose connection failed shows the
//! failure as a selectable error with Report…, which files it filled in.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::resource::pods::PodsPanel;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{PanelScope, REPORT_ERROR_BUTTON};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Modifiers, TestAppContext, VisualTestContext};

const REASON: &str = "unable to run auth exec: No such file or directory";

#[gpui_kit::test]
async fn a_failed_connections_report_files_it_filled_in(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::ui::report_issue::register_handler(cx);
    });
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let connection = cx.new(|_| {
                ClusterConnection::test_with_state(ConnectionState::Failed(REASON.into()))
            });
            let namespaces = cx.new(|_| NamespaceList::empty());
            PodsPanel::with_connection(
                PanelScope::new(NavTarget::pods(), "dev".to_string()),
                connection,
                namespaces,
                cx,
            )
        });
        Root::new(panel, window, cx)
    });
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let report = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(ElementId::Name(REPORT_ERROR_BUTTON.into()))
                .expect("the failure offers Report…")
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(report, Modifiers::none());
    vcx.run_until_parked();

    let (subject, description) = cx
        .update(crate::ui::report_issue::test_fields)
        .expect("Report Issue opened");
    assert_eq!(
        subject, "Couldn't connect to the cluster",
        "no context name"
    );
    assert!(description.contains(REASON), "{description}");
}

/// #252: with no list to carry the context beside its search box, a failed
/// panel still names its context above the failure - outside the error, so
/// Report… (above) still leaves it out.
#[gpui_kit::test]
async fn a_failed_connection_still_names_its_context(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let connection = cx.new(|_| {
                ClusterConnection::test_with_state(ConnectionState::Failed(REASON.into()))
            });
            let namespaces = cx.new(|_| NamespaceList::empty());
            PodsPanel::with_connection(
                PanelScope::new(NavTarget::pods(), "dev".to_string()),
                connection,
                namespaces,
                cx,
            )
        });
        Root::new(panel, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));

    let line = vcx
        .debug_bounds(crate::ui::list_search::CONTEXT_LINE)
        .expect("the context is named");
    assert!(line.size.width > gpui_kit::px(0.), "drawn, not collapsed");
}
