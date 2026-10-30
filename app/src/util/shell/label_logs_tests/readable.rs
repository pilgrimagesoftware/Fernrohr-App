//! #244: a workload's selector reads as words, its `kubectl -l` text in a
//! tooltip, and a typed panel's field explains the forms a selector takes.

use super::window;
use crate::k8s::cluster::connection::ConnectionState;
use crate::ui::nav::NavTarget;
use crate::util::logs::{LabelLogs, SELECTOR_HELP, SELECTOR_INPUT, WORKLOAD_SELECTOR, WorkloadRef};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

/// Hovers element `id` until its tooltip has had time to show; whether the
/// tooltip reading `text` is drawn.
fn tooltip_on_hover(vcx: &mut VisualTestContext, id: &'static str, text: &str) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    let at = vcx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("{id} is drawn"))
        .center();
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    let selector: &'static str = crate::ui::icon_tooltip::selector(text).leak();
    vcx.debug_bounds(selector).is_some()
}

#[gpui_kit::test]
async fn a_workloads_selector_reads_as_words_with_its_text_in_a_tooltip(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (main, mut vcx) = window(ConnectionState::Connecting, cx);
    let target = NavTarget::LabelLogs(LabelLogs::Workload(WorkloadRef {
        kind: "Deployment".into(),
        namespace: "shop".into(),
        name: "web".into(),
        selector: "tier in (back,front)".into(),
    }));
    vcx.update(|window, cx| main.update(cx, |main, cx| main.open_target(target, window, cx)));
    vcx.run_until_parked();

    assert!(
        tooltip_on_hover(
            &mut vcx,
            WORKLOAD_SELECTOR,
            "Label selector: tier in (back,front)"
        ),
        "the selector's own text is in its tooltip"
    );
}

#[gpui_kit::test]
async fn a_typed_panels_field_explains_the_selector_forms(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (main, mut vcx) = window(ConnectionState::Connecting, cx);
    let target = NavTarget::LabelLogs(LabelLogs::new_typed());
    vcx.update(|window, cx| main.update(cx, |main, cx| main.open_target(target, window, cx)));
    vcx.run_until_parked();

    assert!(
        tooltip_on_hover(&mut vcx, SELECTOR_INPUT, SELECTOR_HELP),
        "the field's tooltip lists the forms"
    );
}
