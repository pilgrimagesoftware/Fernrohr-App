//! `0-tab-drop-into-group`: a tab dragged from one group and dropped on
//! another group's tab strip joins that group - including a group of one
//! panel, whose strip gpui-kit 0.7 draws only with `PanelStyle::TabBar`
//! (`util::shell::layout`'s `set_panel_style`). Real mouse events, in a window
//! hosted the way the app hosts one.

use super::close::{Harness, stacked};
use crate::ui::panel::tabs::group_of;
use gpui_kit::component::dock::PanelId;
use gpui_kit::{Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, point};

/// The drawn tab of the panel titled `title`.
fn tab(h: &mut Harness, title: &str) -> Bounds<Pixels> {
    ["focused", "unfocused"]
        .into_iter()
        .find_map(|state| {
            let selector: &'static str = format!("panel-title-{title}-{state}").leak();
            h.vcx.debug_bounds(selector)
        })
        .unwrap_or_else(|| panic!("{title}'s tab is drawn"))
}

/// Presses at `from`, moves there to `to` in steps, and releases.
fn drag(h: &mut Harness, from: Point<Pixels>, to: Point<Pixels>) {
    h.vcx
        .simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for step in 1..=10 {
        let f = step as f32 / 10.;
        let at = point(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
        h.vcx
            .simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    h.vcx
        .simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    h.vcx.run_until_parked();
}

fn same_group(h: &mut Harness, a: PanelId, b: PanelId) -> bool {
    let dock = h.dock();
    h.vcx.update(|_, cx| {
        let area = dock.read(cx);
        group_of(area, a).is_some() && group_of(area, a) == group_of(area, b)
    })
}

/// The user's case: the lower group holds RoleBindings alone. Dropping Pods'
/// tab on RoleBindings' tab adds Pods to that group.
#[gpui_kit::test]
async fn a_tab_dropped_on_a_lone_groups_tab_joins_that_group(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let (pods, bindings) = stacked(&mut h);
    assert!(!same_group(&mut h, pods, bindings), "two groups to start");

    let (from, to) = (
        tab(&mut h, "Pods").center(),
        tab(&mut h, "Rolebindings").center(),
    );
    drag(&mut h, from, to);

    assert!(
        same_group(&mut h, pods, bindings),
        "Pods joined RoleBindings' group"
    );
}

/// The same drop on a group of two panels, which always had a strip.
#[gpui_kit::test]
async fn a_tab_dropped_on_a_two_tab_strip_joins_that_group(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let (pods, bindings) = stacked(&mut h);
    h.focus(bindings);
    let services = h.open_list("services");
    assert!(
        same_group(&mut h, services, bindings),
        "Services opened beside RoleBindings"
    );

    let (from, to) = (
        tab(&mut h, "Pods").center(),
        tab(&mut h, "Services").center(),
    );
    drag(&mut h, from, to);

    assert!(
        same_group(&mut h, pods, services),
        "Pods joined the lower group"
    );
}
