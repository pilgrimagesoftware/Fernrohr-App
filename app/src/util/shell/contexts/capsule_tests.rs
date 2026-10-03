//! `toolbar-layout-with-gpui-kit` section 1 through a real window (a `Root`, the
//! app's keymap): the contexts' capsules sit in the status bar at the window's
//! bottom edge and nowhere else, and add and disconnect are reached from them -
//! by the capsule's controls and by the palette commands.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::status_bar::{AddContext, DisconnectActiveContext};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};

struct Harness {
    vcx: VisualTestContext,
    main: Entity<MainWindow>,
}

fn harness(cx: &mut TestAppContext, contexts: &[&str]) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        for context in contexts {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
    });
    let names: Vec<String> = contexts.iter().map(|name| name.to_string()).collect();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(names, window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    Harness { vcx, main }
}

fn capsules(h: &mut Harness) -> Vec<String> {
    h.vcx
        .update(|_, cx| match h.main.read(cx).test_status_bar() {
            Some(bar) => bar
                .read(cx)
                .items(cx)
                .into_iter()
                .map(|item| item.context_name)
                .collect(),
            None => Vec::new(),
        })
}

fn dialog_open(h: &mut Harness) -> bool {
    h.vcx.update(|window, cx| window.has_active_dialog(cx))
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

/// Confirms the open Disconnect dialog from the keyboard: Tab past Cancel to
/// Disconnect, then Space presses it. A pointer click races the dialog's open
/// animation on a slow machine; the keyboard route doesn't, and it's the one the
/// keyboard-first rule asks to be tested.
fn confirm_by_keyboard(h: &mut Harness) {
    for _ in 0..2 {
        h.vcx.simulate_keystrokes("tab");
        h.vcx.run_until_parked();
    }
    let space = gpui_kit::Keystroke::parse("space").expect("valid");
    h.vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    h.vcx
        .simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    h.vcx.run_until_parked();
}

/// 1.1 and 1.4: both contexts' capsules are drawn along the window's bottom edge -
/// the status bar - and nothing draws them at the top, where the context bar was.
#[gpui_kit::test]
async fn the_capsules_are_in_the_status_bar_at_the_bottom(cx: &mut TestAppContext) {
    let mut h = harness(cx, &["kind-dev", "staging"]);
    assert_eq!(capsules(&mut h), ["kind-dev", "staging"]);
    h.vcx.update(|window, cx| window.render_frame(cx));
    let height = h.vcx.update(|window, _| window.bounds().size.height);
    for name in ["kind-dev", "staging"] {
        let selector: &'static str = format!("status-item-{name}").leak();
        let drawn = h
            .vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{name} is drawn"));
        assert!(
            drawn.top() > height * 0.9,
            "{name}'s capsule is in the bottom status bar: {drawn:?}"
        );
    }
}

/// 1.2: the add control after the capsules, and its palette command, each open the
/// add popover.
#[gpui_kit::test]
async fn the_add_control_and_command_open_the_add_popover(cx: &mut TestAppContext) {
    let mut h = harness(cx, &["kind-dev"]);
    click(&mut h, "context-bar-add");
    assert!(dialog_open(&mut h), "the control opens it");
    h.vcx.update(|window, cx| window.close_all_dialogs(cx));
    h.vcx.run_until_parked();

    h.vcx.dispatch_action(AddContext);
    h.vcx.run_until_parked();
    assert!(dialog_open(&mut h), "the command opens it");
    h.vcx.update(|window, cx| window.close_all_dialogs(cx));
}

/// 1.3: Disconnect, by its command, asks first; confirming removes the capsule,
/// and disconnecting the window's last context returns it to the picker.
#[gpui_kit::test]
async fn disconnect_confirms_then_removes_the_capsule_and_the_last_returns_to_the_picker(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx, &["kind-dev", "staging"]);
    h.vcx.dispatch_action(DisconnectActiveContext);
    h.vcx.run_until_parked();
    assert!(dialog_open(&mut h), "it asks first");
    assert_eq!(capsules(&mut h).len(), 2, "nothing closed yet");
    confirm_by_keyboard(&mut h);
    assert_eq!(capsules(&mut h), ["staging"]);

    h.vcx.dispatch_action(DisconnectActiveContext);
    h.vcx.run_until_parked();
    confirm_by_keyboard(&mut h);
    assert!(
        h.vcx
            .update(|_, cx| h.main.read(cx).test_status_bar())
            .is_none(),
        "the window is back on the picker"
    );
}

/// 1.3: the capsule's own menu offers Disconnect too.
#[gpui_kit::test]
async fn a_capsules_menu_offers_disconnect(cx: &mut TestAppContext) {
    let mut h = harness(cx, &["kind-dev", "staging"]);
    click(&mut h, "context-chip-menu-staging");
    // The menu's one item is Disconnect: Enter picks it.
    h.vcx.simulate_keystrokes("enter");
    h.vcx.run_until_parked();
    assert!(dialog_open(&mut h), "the menu's Disconnect asks first");
    confirm_by_keyboard(&mut h);
    assert_eq!(capsules(&mut h), ["kind-dev"], "staging's capsule went");
}
