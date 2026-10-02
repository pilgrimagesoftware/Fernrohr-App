//! The Resource panel's move, collapse and focus, driven through a real window:
//! keystrokes from the live keymap and clicks on its drawn buttons, not handler
//! calls - the path the 11.2-11.3 tests skipped, which is how the missing controls
//! went unnoticed. 11.1's preference is read from and saved to a temp `ui.toml`.

// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget.
use crate::command::{CommandRegistry, MenuSlot};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::keymap::{KeymapConfig, conflicts};
use crate::ui::resource_panel::ResourceSide;
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, init, register_commands};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};

struct Harness {
    vcx: VisualTestContext,
    main: Entity<MainWindow>,
}

/// A workspace window the way the app opens one - `shell::init`'s keymap, inside a
/// `Root` - with Pods listed in its Resource panel and focus on the window.
fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    open(cx)
}

/// Another workspace window in the same app, as `harness` builds it.
fn open(cx: &mut TestAppContext) -> Harness {
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    window
        .update(cx, |_, window, cx| {
            main.update(cx, |main, cx| {
                main.test_resource_panel()
                    .expect("a workspace has a Resource panel")
                    .update(cx, |panel, cx| {
                        panel.test_show_kinds(vec![DiscoveredKind::pods()], cx)
                    });
            });
            let focus = main.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    Harness { vcx, main }
}

fn layout(h: &mut Harness) -> (ResourceSide, bool) {
    h.vcx
        .update(|_, cx| h.main.read(cx).test_resource_layout().expect("a workspace"))
}

/// Where the Pods row is drawn, if it is.
fn row(h: &mut Harness) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds("resource-row-Workloads-0")
}

fn panel_focused(h: &mut Harness) -> bool {
    h.vcx.update(|window, cx| {
        h.main
            .read(cx)
            .test_resource_panel()
            .unwrap()
            .read(cx)
            .focus_handle()
            .contains_focused(window, cx)
    })
}

fn click(h: &mut Harness, id: &'static str) {
    let center = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .unwrap_or_else(|| panic!("{id} is drawn"))
            .bounds()
            .center()
    });
    h.vcx.simulate_click(center, Modifiers::none());
    h.vcx.run_until_parked();
}

/// 11.2: the move key and the header button each put the panel on the other edge,
/// and it draws there - the dock on its other side.
#[gpui_kit::test]
async fn the_move_key_and_button_put_the_panel_on_the_other_edge(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    assert_eq!(
        layout(&mut h),
        (ResourceSide::Left, false),
        "a new window: left, expanded"
    );
    let window_width = h.vcx.update(|window, _| window.bounds().size.width);
    let on_left = row(&mut h).expect("the row is drawn");
    assert!(
        on_left.right() < window_width / 2.,
        "drawn on the left: {on_left:?}"
    );

    press(&mut h.vcx, crate::ui::resource_panel::MOVE_DEFAULT_BINDING);
    assert_eq!(layout(&mut h).0, ResourceSide::Right);
    let on_right = row(&mut h).expect("the row is drawn");
    assert!(
        on_right.left() > window_width / 2.,
        "drawn on the right: {on_right:?}"
    );

    click(&mut h, "resource-panel-move");
    assert_eq!(
        layout(&mut h).0,
        ResourceSide::Left,
        "the button moves it back"
    );
    assert!(row(&mut h).unwrap().right() < window_width / 2.);
}

/// 11.3: the collapse key hides the panel down to its strip and takes focus off it;
/// the strip's button brings it back focused, and the header button collapses it.
#[gpui_kit::test]
async fn the_collapse_key_and_buttons_hide_and_restore_the_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    click(&mut h, "resource-row-Workloads-0");
    assert!(panel_focused(&mut h), "a click on a row focuses the panel");

    press(
        &mut h.vcx,
        crate::ui::resource_panel::TOGGLE_DEFAULT_BINDING,
    );
    assert!(layout(&mut h).1, "collapsed");
    assert!(row(&mut h).is_none(), "its rows aren't drawn");
    assert!(!panel_focused(&mut h), "focus left the hidden panel");

    click(&mut h, "resource-panel-expand");
    assert!(!layout(&mut h).1, "the strip's button expands it");
    assert!(row(&mut h).is_some());
    assert!(panel_focused(&mut h), "and focuses it");

    click(&mut h, "resource-panel-collapse");
    assert!(layout(&mut h).1, "the header's button collapses it");
}

/// Focus Resources (cmd-0) on a collapsed panel brings it back, focused; Next Panel
/// skips it while it's collapsed.
#[gpui_kit::test]
async fn focusing_a_collapsed_panel_expands_it_and_panel_stepping_skips_it(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx);
    press(
        &mut h.vcx,
        crate::ui::resource_panel::TOGGLE_DEFAULT_BINDING,
    );
    press(
        &mut h.vcx,
        crate::ui::panel::focus::FOCUS_NEXT_DEFAULT_BINDING,
    );
    assert!(!panel_focused(&mut h), "a collapsed panel is no stop");

    press(&mut h.vcx, "cmd-0");
    assert!(!layout(&mut h).1, "Focus Resources expands it");
    assert!(panel_focused(&mut h));
}

/// (b): clicking the panel focuses it, shows the focus indicator, and its keys then
/// work - Space toggles the section under the cursor, as Left does.
#[gpui_kit::test]
async fn a_clicked_panel_shows_focus_and_its_keys_work(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let indicator = |h: &mut Harness, state: &'static str| {
        h.vcx.update(|window, cx| window.render_frame(cx));
        h.vcx.debug_bounds(state).is_some()
    };
    assert!(indicator(&mut h, "resource-panel-unfocused"));

    click(&mut h, "resource-row-Workloads-0");
    assert!(panel_focused(&mut h));
    assert!(
        indicator(&mut h, "resource-panel-focused"),
        "the panel shows it has focus"
    );

    let pods = DiscoveredKind::pods();
    let collapsed = |h: &mut Harness| {
        h.vcx.update(|_, cx| {
            h.main
                .read(cx)
                .test_resource_panel()
                .unwrap()
                .read(cx)
                .is_section_collapsed(&pods)
        })
    };
    press(&mut h.vcx, "space");
    assert!(collapsed(&mut h), "Space collapses the cursor's section");
    press(&mut h.vcx, "space");
    assert!(!collapsed(&mut h), "and expands it again");
    press(&mut h.vcx, "left");
    assert!(collapsed(&mut h), "Left collapses it too");
}

/// 11.1: a new window opens its panel on the stored edge, read from `ui.toml`.
#[gpui_kit::test]
async fn a_new_window_opens_the_panel_on_the_preferred_edge(cx: &mut TestAppContext) {
    let path = temp_workspace_path();
    let stored = crate::config::ui::UiConfig {
        resource_side: ResourceSide::Right,
        ..Default::default()
    };
    crate::config::save(&path, &stored).expect("temp file written");
    cx.update(|cx| {
        let loaded: crate::config::ui::UiConfig = crate::config::load(&path);
        crate::ui::resource_panel::init_side_preference(loaded.resource_side, path.clone(), cx);
    });
    let mut h = harness(cx);
    assert_eq!(layout(&mut h), (ResourceSide::Right, false));
    let window_width = h.vcx.update(|window, _| window.bounds().size.width);
    let drawn = row(&mut h).expect("the row is drawn");
    assert!(
        drawn.left() > window_width / 2.,
        "drawn on the right: {drawn:?}"
    );
}

/// 11.2 against 11.1: moving the panel leaves the preference alone, so the next
/// window still opens on the preferred edge - until Make Resource Panel's Side the
/// Default saves the moved window's side, which new windows then follow.
#[gpui_kit::test]
async fn moving_keeps_the_preference_and_saving_the_side_changes_it(cx: &mut TestAppContext) {
    let path = temp_workspace_path();
    cx.update(|cx| {
        crate::ui::resource_panel::init_side_preference(ResourceSide::Left, path.clone(), cx)
    });
    let mut first = harness(cx);
    press(
        &mut first.vcx,
        crate::ui::resource_panel::MOVE_DEFAULT_BINDING,
    );
    assert_eq!(layout(&mut first).0, ResourceSide::Right);
    let mut second = open(cx);
    assert_eq!(
        layout(&mut second).0,
        ResourceSide::Left,
        "a move isn't a preference"
    );

    first
        .vcx
        .dispatch_action(crate::ui::resource_panel::SaveResourceSide);
    first.vcx.run_until_parked();
    let saved: crate::config::ui::UiConfig = crate::config::load(&path);
    assert_eq!(
        saved.resource_side,
        ResourceSide::Right,
        "written to ui.toml"
    );
    assert_eq!(
        layout(&mut second).0,
        ResourceSide::Left,
        "an open window stays put"
    );
    let mut third = open(cx);
    assert_eq!(layout(&mut third).0, ResourceSide::Right);
}

/// All three are palette commands with a View menu item, and no default key
/// collides with another command's.
#[test]
fn all_three_are_view_menu_commands_whose_keys_collide_with_nothing() {
    use crate::ui::resource_panel::{MOVE_COMMAND_ID, SAVE_SIDE_COMMAND_ID, TOGGLE_COMMAND_ID};
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let view: Vec<&str> = registry
        .for_menu(MenuSlot::View)
        .iter()
        .map(|command| command.id)
        .collect();
    assert!(view.contains(&SAVE_SIDE_COMMAND_ID), "in the View menu");
    for id in [TOGGLE_COMMAND_ID, MOVE_COMMAND_ID] {
        assert!(view.contains(&id), "{id} in the View menu");
        let keys = registry.get(id).unwrap().default_binding;
        let found = conflicts(&registry, &KeymapConfig::default(), id, keys);
        assert!(found.same_scope.is_empty(), "{id} ({keys}): {found:?}");
    }
}
