//! `visual-refresh-typography-spacing` 3.2, tables: the Resource panel's
//! list sits at least the panel inset from the panel's edge.

use super::{kind, stub_panel};
use crate::ui::space::{TextScale, spacing};
use gpui_kit::{TestAppContext, VisualTestContext};

/// At 150% text, so the inset is the scaled token, not the 8px it replaced.
#[gpui_kit::test]
async fn the_kind_list_is_inset_from_the_panel_edge(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        TextScale::new(1.5).expect("a valid scale").set(cx);
    });
    let window = stub_panel(cx);
    window
        .update(cx, |panel, _window, cx| {
            panel.state = super::super::ResourceState::Loaded(vec![kind("", "Pod")]);
            cx.notify();
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let inset = vcx.update(|_, cx| spacing(cx).panel_inset);
    let row = vcx
        .debug_bounds("resource-row-Workloads-0")
        .expect("the Pod row is drawn");
    assert!(
        row.left() >= inset,
        "the Pod row starts {:?} from the panel edge, under {inset:?}",
        row.left()
    );
}
