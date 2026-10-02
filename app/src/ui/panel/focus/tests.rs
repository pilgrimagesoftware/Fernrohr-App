//! Tests for panel focus order: `step`'s wrapping, and `dock_stops` over real
//! layouts in the app's own `DockSkin` dock - splits, a side and bottom dock, a
//! hidden tab, a collapsed dock and a zoomed group.

use super::{Direction, dock_stops, step};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::ui::placeholder::PlaceholderPanel;
use gpui_kit::component::dock::{
    DockArea, DockLayout, DockPlacement, DockSkin, PanelId, panel_handle,
};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div,
};
use kube::core::GroupVersionKind;
use std::rc::Rc;

#[test]
fn next_and_previous_wrap_at_both_ends() {
    assert_eq!(step(3, Some(0), Direction::Next), Some(1));
    assert_eq!(
        step(3, Some(2), Direction::Next),
        Some(0),
        "last wraps to first"
    );
    assert_eq!(
        step(3, Some(0), Direction::Previous),
        Some(2),
        "first wraps to last"
    );
    assert_eq!(step(3, Some(1), Direction::Previous), Some(0));
}

#[test]
fn previous_undoes_next() {
    for len in 1..5 {
        for start in 0..len {
            let there = step(len, Some(start), Direction::Next);
            assert_eq!(step(len, there, Direction::Previous), Some(start));
        }
    }
}

#[test]
fn with_nothing_focused_next_starts_first_and_previous_last() {
    assert_eq!(step(3, None, Direction::Next), Some(0));
    assert_eq!(step(3, None, Direction::Previous), Some(2));
}

#[test]
fn one_stop_stays_put_and_none_goes_nowhere() {
    assert_eq!(step(1, Some(0), Direction::Next), Some(0));
    assert_eq!(step(1, Some(0), Direction::Previous), Some(0));
    assert_eq!(step(0, None, Direction::Next), None);
    assert_eq!(step(0, None, Direction::Previous), None);
}

/// The app's dock, hosting whatever layout a test sets.
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
        let (area, skin) = DockSkin::dock_area("focus-order", Some(1), window, cx);
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

fn tabs(panels: &[&Entity<PlaceholderPanel>], cx: &mut VisualTestContext) -> DockLayout {
    cx.update(|_window, cx| {
        panels.iter().fold(DockLayout::tabs(), |layout, panel| {
            layout.panel_view(panel_handle((*panel).clone()), cx)
        })
    })
}

fn stops(area: &Entity<DockArea>, cx: &mut VisualTestContext) -> Vec<PanelId> {
    cx.update(|_window, cx| dock_stops(area.read(cx), cx))
}

/// The panels a test layout is built from: a left dock, a center split of two
/// groups (the second showing its *second* tab), and a bottom dock.
struct Layout {
    left: Entity<PlaceholderPanel>,
    first: Entity<PlaceholderPanel>,
    behind: Entity<PlaceholderPanel>,
    shown: Entity<PlaceholderPanel>,
    bottom: Entity<PlaceholderPanel>,
}

fn build(area: &Entity<DockArea>, cx: &mut VisualTestContext) -> Layout {
    let layout = Layout {
        left: panel("Left", cx),
        first: panel("First", cx),
        behind: panel("Behind", cx),
        shown: panel("Shown", cx),
        bottom: panel("Bottom", cx),
    };
    let left = tabs(&[&layout.left], cx);
    let first = tabs(&[&layout.first], cx);
    let second = tabs(&[&layout.behind, &layout.shown], cx).active_index(1);
    let bottom = tabs(&[&layout.bottom], cx);
    let center = DockLayout::h_split().child(first, None).child(second, None);
    cx.update(|window, cx| {
        area.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Left, left, window, cx);
            area.set_dock(DockPlacement::Bottom, bottom, window, cx);
            for placement in [DockPlacement::Left, DockPlacement::Bottom] {
                if !area.is_dock_open(placement) {
                    area.toggle_dock(placement, window, cx);
                }
            }
        });
    });
    cx.run_until_parked();
    layout
}

/// Left dock, then the center split left to right - one stop per group, its
/// displayed tab, so a tab behind another is skipped - then the bottom dock.
#[gpui_kit::test]
fn stops_follow_reading_order_one_per_group(cx: &mut TestAppContext) {
    let (area, cx) = host(cx);
    let layout = build(&area, cx);

    assert_eq!(
        stops(&area, cx),
        vec![
            id(&layout.left),
            id(&layout.first),
            id(&layout.shown),
            id(&layout.bottom)
        ],
    );
    assert!(
        !stops(&area, cx).contains(&id(&layout.behind)),
        "a tab behind another in its group is not a stop"
    );
}

/// A collapsed dock still lists its panels in the layout, but they aren't on
/// screen, so they aren't stops.
#[gpui_kit::test]
fn a_collapsed_dock_is_skipped(cx: &mut TestAppContext) {
    let (area, cx) = host(cx);
    let layout = build(&area, cx);
    cx.update(|window, cx| {
        area.update(cx, |area, cx| {
            area.toggle_dock(DockPlacement::Left, window, cx)
        });
    });
    cx.run_until_parked();

    assert!(cx.update(|_window, cx| !area.read(cx).is_dock_open(DockPlacement::Left)));
    assert_eq!(
        stops(&area, cx),
        vec![id(&layout.first), id(&layout.shown), id(&layout.bottom)],
    );
}

/// While a group is zoomed it is all the dock draws, so its displayed panel is
/// the dock's only stop.
#[gpui_kit::test]
fn a_zoomed_group_is_the_only_dock_stop(cx: &mut TestAppContext) {
    let (area, cx) = host(cx);
    let layout = build(&area, cx);
    let shown = id(&layout.shown);
    cx.update(|window, cx| {
        area.update(cx, |area, cx| {
            let node = area
                .layout(DockPlacement::Center)
                .and_then(|tree| tree.find_panel_node(shown))
                .expect("the shown panel is in the center");
            area.set_zoomed_in(node, window, cx);
        });
    });
    cx.run_until_parked();

    assert!(cx.update(|_window, cx| area.read(cx).is_zoomed()));
    assert_eq!(stops(&area, cx), vec![shown]);
}

/// Both commands take their keys from `keymap.toml` by id like any other
/// command: an override replaces the default rather than adding to it.
#[test]
fn keymap_overrides_rebind_both_commands() {
    use super::{FocusNextPanel, FocusPreviousPanel, register_commands};
    use crate::command::CommandRegistry;
    use crate::keymap::{self, KeymapConfig};
    use gpui_kit::{Action, Keymap, Keystroke};

    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let mut config = KeymapConfig::default();
    config
        .bindings
        .insert("panel.focus_next".into(), "ctrl-j".into());
    config
        .bindings
        .insert("panel.focus_previous".into(), "ctrl-k".into());
    let keymap = Keymap::new(keymap::bindings(
        &registry,
        &config,
        &gpui_kit::DummyKeyboardMapper,
    ));
    let bound = |key: &str, action: &dyn Action| {
        let keystroke = Keystroke::parse(key).expect("a valid keystroke");
        let (matched, _) = keymap.bindings_for_input(std::slice::from_ref(&keystroke), &[]);
        matched
            .iter()
            .any(|binding| binding.action().partial_eq(action))
    };

    assert!(bound("ctrl-j", &FocusNextPanel), "the override binds next");
    assert!(
        bound("ctrl-k", &FocusPreviousPanel),
        "the override binds previous"
    );
    assert!(!bound("cmd-]", &FocusNextPanel), "and replaces the default");
    assert!(!bound("cmd-[", &FocusPreviousPanel), "for both commands");
}
