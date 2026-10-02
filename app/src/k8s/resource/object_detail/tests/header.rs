//! `resource-kind-icons` 3.5: the object viewer's header leads the object's
//! name with its own kind's icon, at the header size.

use super::fixtures::{deployments, owned_replica_set, replica_sets, stub_panel, target};
use gpui_kit::{TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn the_header_leads_with_a_header_size_icon_of_the_objects_kind(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let (window, panel) = stub_panel(
        cx,
        target(replica_sets(), Some("staging"), "web-7d9f"),
        vec![replica_sets(), deployments()],
    );
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(owned_replica_set(), cx)
            })
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    crate::ui::panel_title::test_support::assert_header_icon_leads_at_every_text_size(&mut vcx);
    assert!(
        vcx.debug_bounds("kind-icon-ReplicaSet").is_some(),
        "the header's icon is the object's own kind's, ReplicaSet's"
    );
}
