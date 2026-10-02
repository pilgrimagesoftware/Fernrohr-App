//! Tests for tab targeting and the focused tab group, over real layouts in the
//! app's own `DockSkin` dock.

use super::{TabGroup, TabTarget, focused_group, target_index};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::ui::placeholder::PlaceholderPanel;
use gpui_kit::component::dock::{
    DockArea, DockLayout, DockPlacement, DockSkin, PanelId, panel_handle,
};
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div,
};
use kube::core::GroupVersionKind;
use std::rc::Rc;

#[test]
fn next_and_previous_wrap_within_the_group() {
    assert_eq!(target_index(3, 0, TabTarget::Next), Some(1));
    assert_eq!(target_index(3, 2, TabTarget::Next), Some(0));
    assert_eq!(target_index(3, 0, TabTarget::Previous), Some(2));
    assert_eq!(
        target_index(1, 0, TabTarget::Next),
        Some(0),
        "one tab stays put"
    );
}

#[test]
fn positions_index_directly_and_nine_is_the_last_tab() {
    assert_eq!(target_index(5, 0, TabTarget::Position(3)), Some(2));
    assert_eq!(
        target_index(4, 0, TabTarget::Position(9)),
        Some(3),
        "9 = last"
    );
    assert_eq!(target_index(12, 0, TabTarget::Position(9)), Some(11));
    assert_eq!(
        target_index(2, 0, TabTarget::Position(5)),
        None,
        "past the end"
    );
    assert_eq!(
        target_index(0, 0, TabTarget::Position(1)),
        None,
        "empty group"
    );
}

struct DockHost {
    area: Entity<DockArea>,
    _skin: Rc<DockSkin>,
}

impl Render for DockHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.area.clone())
    }
}

fn host(cx: &mut TestAppContext) -> (Entity<DockArea>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let (host, cx) = cx.add_window_view(|window, cx| {
        let (area, skin) = DockSkin::dock_area("tab-groups", Some(1), window, cx);
        DockHost { area, _skin: skin }
    });
    let area = cx.update(|_window, cx| host.read(cx).area.clone());
    (area, cx)
}

fn panel(name: &str, cx: &mut VisualTestContext) -> Entity<PlaceholderPanel> {
    let kind = DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", name),
        plural: format!("{name}s"),
        namespaced: true,
        verbs: Default::default(),
    };
    cx.update(|_window, cx| {
        cx.new(|cx| {
            PlaceholderPanel::with_namespaces(
                kind.clone(),
                PanelScope::new(NavTarget::Kind(kind), "kind-dev".into()),
                cx.new(|_| NamespaceList::empty()),
                cx,
            )
        })
    })
}

fn id(panel: &Entity<PlaceholderPanel>) -> PanelId {
    PanelId::from(panel.entity_id())
}

fn group(area: &Entity<DockArea>, cx: &mut VisualTestContext) -> Option<TabGroup> {
    cx.update(|window, cx| focused_group(area.read(cx), window, cx))
}

/// Center: one group of two tabs (the second displayed). Right dock: one tab.
fn build(area: &Entity<DockArea>, cx: &mut VisualTestContext) -> [Entity<PlaceholderPanel>; 3] {
    let [first, second, right] = [panel("First", cx), panel("Second", cx), panel("Right", cx)];
    let (center, side) = cx.update(|_window, cx| {
        (
            DockLayout::tabs()
                .panel_view(panel_handle(first.clone()), cx)
                .panel_view(panel_handle(second.clone()), cx)
                .active_index(1),
            DockLayout::tabs().panel_view(panel_handle(right.clone()), cx),
        )
    });
    cx.update(|window, cx| {
        area.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Right, side, window, cx);
            if !area.is_dock_open(DockPlacement::Right) {
                area.toggle_dock(DockPlacement::Right, window, cx);
            }
        });
    });
    cx.run_until_parked();
    [first, second, right]
}

/// Focus inside a group picks that group, whichever region it's in.
#[gpui_kit::test]
fn the_focused_group_is_the_one_focus_is_in(cx: &mut TestAppContext) {
    let (area, cx) = host(cx);
    let [first, second, right] = build(&area, cx);

    cx.update(|window, cx| right.read(cx).focus_handle(cx).focus(window, cx));
    cx.run_until_parked();
    assert_eq!(
        group(&area, cx),
        Some(TabGroup {
            panels: vec![id(&right)],
            active_ix: 0
        })
    );

    cx.update(|window, cx| second.read(cx).focus_handle(cx).focus(window, cx));
    cx.run_until_parked();
    assert_eq!(
        group(&area, cx),
        Some(TabGroup {
            panels: vec![id(&first), id(&second)],
            active_ix: 1
        })
    );
}

/// With focus outside every group, the first group in panel-focus order is
/// meant - here the center, before the right dock.
#[gpui_kit::test]
fn with_focus_outside_the_dock_the_first_group_is_meant(cx: &mut TestAppContext) {
    let (area, cx) = host(cx);
    let [first, second, _right] = build(&area, cx);

    assert_eq!(
        group(&area, cx),
        Some(TabGroup {
            panels: vec![id(&first), id(&second)],
            active_ix: 1
        })
    );
}

/// A dock with nothing on screen has no focused group.
#[gpui_kit::test]
fn an_empty_dock_has_no_focused_group(cx: &mut TestAppContext) {
    let (area, cx) = host(cx);
    assert_eq!(group(&area, cx), None);
}
