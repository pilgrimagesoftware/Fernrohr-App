//! Section 5.2: a restored panel whose namespace or object no longer exists
//! needs no saved-layout-specific logic - it already uses the panel kind's
//! own existing not-found handling, the same as it would if the object
//! vanished while the panel was open normally. This test is the "Verify"
//! task 5.2 itself asks for: a pod detail panel restored by Replace, pointed
//! at a pod a fake cluster (`k8s::test_cluster::FakeCluster`) never served,
//! settles on its own existing `PodDetailState::NotFound` ("this pod no
//! longer exists") without erroring, closing, or affecting the rest of the
//! restored layout.
//!
//! Named imports from `super`, not `use super::*`: see `tests.rs`'s own doc
//! comment for why.
use super::{fixture_layout, harness_with_connection, open_picker, open_targets};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::pod_detail::PodDetailPanel;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::util::shell::test_support::press;
use gpui_kit::{Entity, TestAppContext};

/// 5.2: a saved pod detail panel for a pod the cluster doesn't serve
/// restores normally (no content-key gap, no placeholder - it's a known
/// kind, scoped to a context this window holds) and settles on its own
/// "pod no longer exists" state once its fetch lands, while the rest of the
/// restored layout - Pods - is unaffected.
#[gpui_kit::test]
async fn a_restored_pod_detail_for_a_vanished_pod_shows_its_own_not_found_state(
    cx: &mut TestAppContext,
) {
    // `FakeCluster::start` needs the tokio runtime up first - `harness_with_connection`
    // (via `crate::util::shell::init`) brings it up too, but only after it's
    // already built the window, which is too late for this.
    cx.update(crate::runtime::init);
    let (_cluster, client) = FakeCluster::start(cx);
    let mut h = harness_with_connection(cx, ConnectionState::Connected(client));
    let gone = NavTarget::pod("shop", "gone");
    let fixture = fixture_layout(cx, "VanishedPod", vec!["demo".into()], vec![gone.clone()]);
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods(), gone.clone()],
        "both panels restored with their content keys - a vanished pod is \
         not a saved-layout-level problem"
    );

    let main = h.main.clone();
    let view = h
        .vcx
        .update(move |_window, cx| main.read(cx).test_panel_view_for(&gone, cx))
        .expect("the restored pod detail panel is in the dock");
    let panel: Entity<PodDetailPanel> = Entity::from(view.as_ref());

    // The fetch runs on the real tokio runtime; poll until it lands or fail
    // loudly rather than hang (mirrors `pod_detail::tests::live`'s own
    // `wait_for`).
    let mut settled = false;
    for _ in 0..400 {
        h.vcx.run_until_parked();
        if h.vcx
            .update(|_window, cx| panel.read(cx).test_is_not_found())
        {
            settled = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        settled,
        "the restored pod detail panel reached its own not-found state"
    );
    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods(), NavTarget::pod("shop", "gone")],
        "the not-found pod detail panel is still open, not closed"
    );
}
