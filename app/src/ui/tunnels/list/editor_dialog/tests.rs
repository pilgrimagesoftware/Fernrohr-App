//! Fernrohr#165 in a real Tunnels window under a `Root`, from the keyboard: the
//! editor opens as a dialog with focus on its first field; in a window too short
//! for the command form, Tab still reaches the startup timeout and the dialog
//! scrolls it into view with Save still showing; Enter in the command keeps a
//! newline; Escape keeps the tunnel as it was and Enter saves it, each closing the
//! dialog and handing focus back to the tunnel's row; a save that fails validation
//! stays open.

// Named imports: a glob of `gpui_kit::*` beside `#[gpui_kit::test]` shadows the
// built-in `#[test]`.
use super::{CANCEL_ID, SAVE_ID};
use crate::config::tunnels::{TunnelConfig, TunnelKind};
use crate::tunnel::store::TunnelStore;
use crate::ui::tunnels::editor::TIMEOUT_FIELD_SELECTOR;
use crate::ui::tunnels::list::TunnelsWindow;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, px, size,
};
use std::path::PathBuf;

const TUNNEL_ID: &str = "qa-bastion";

struct Harness {
    vcx: VisualTestContext,
    view: Entity<TunnelsWindow>,
    path: PathBuf,
}

/// A Tunnels window `height` tall, listing one SSH tunnel, "QA".
fn harness(cx: &mut TestAppContext, height: f32) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let path = crate::util::test_paths::temp_path("tunnel-editor-dialog");
    TunnelStore::new(path.clone())
        .create(
            TUNNEL_ID,
            TunnelConfig {
                name: "QA".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                ..Default::default()
            },
            None,
        )
        .expect("the fixture tunnel saves");
    let kubeconfig = std::env::temp_dir().join("fernrohr-editor-dialog-no-such-kubeconfig.yaml");
    let mut built = None;
    let window = cx.add_window({
        let path = path.clone();
        |window, cx| {
            let view = cx.new(|cx| TunnelsWindow::new(path, Some(kubeconfig), window, cx));
            built = Some(view.clone());
            gpui_kit::component::Root::new(view, window, cx)
        }
    });
    let view = built.expect("the window built its view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_resize(size(px(640.), px(height)));
    let harness = Harness { vcx, view, path };
    harness.vcx.run_until_parked();
    harness
}

impl Harness {
    fn click(&mut self, id: &str) {
        let id: &'static str = id.to_string().leak();
        let at = self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        });
        self.vcx.simulate_click(at, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn keys(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    /// Space down and up: a `Button` clicks on the key's release.
    fn press_space(&mut self) {
        let space = gpui_kit::Keystroke::parse("space").expect("valid");
        self.vcx.simulate_event(gpui_kit::KeyDownEvent {
            keystroke: space.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        self.vcx
            .simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
        self.vcx.run_until_parked();
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    /// The editor field holding focus, by label.
    fn focused_field(&mut self) -> Option<&'static str> {
        let view = self.view.clone();
        self.vcx.update(|window, cx| {
            let editor = view.read(cx).editor.clone()?;
            editor.read(cx).focused_field(window, cx)
        })
    }

    fn editor_kind(&mut self) -> Option<TunnelKind> {
        let view = self.view.clone();
        self.vcx.update(|_, cx| {
            let editor = view.read(cx).editor.clone()?;
            Some(editor.read(cx).kind())
        })
    }

    fn row_focused(&mut self) -> bool {
        let view = self.view.clone();
        self.vcx.update(|window, cx| {
            view.read(cx)
                .row_focus
                .get(TUNNEL_ID)
                .is_some_and(|focus| focus.is_focused(window))
        })
    }

    fn saved_name(&self) -> String {
        TunnelStore::new(self.path.clone())
            .get(TUNNEL_ID)
            .expect("the tunnel is still stored")
            .name
    }

    /// Where `selector` draws this frame, and the window's own bounds.
    fn drawn(&mut self, selector: &'static str) -> (Bounds<Pixels>, Bounds<Pixels>) {
        self.vcx.update(|window, cx| window.render_frame(cx));
        let at = self
            .vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        let viewport = self
            .vcx
            .update(|window, _| Bounds::new(Default::default(), window.viewport_size()));
        (at, viewport)
    }

    fn edit(&mut self) {
        self.click(&format!("tunnels-edit-{TUNNEL_ID}"));
        assert!(self.dialog_open(), "Edit opens the editor as a dialog");
        assert_eq!(self.focused_field(), Some("Name"), "focus starts on Name");
    }
}

/// The command form is taller than a short window: switching to Command from the
/// keyboard, Tab still reaches the startup timeout - past the multi-line command,
/// where Enter adds a line rather than saving - and the dialog scrolls it into
/// view, with Save still on screen.
#[gpui_kit::test]
async fn in_a_short_window_tab_reaches_the_command_forms_timeout(cx: &mut TestAppContext) {
    let mut h = harness(cx, 360.);
    h.edit();

    // Back past Manual, the last kind, to Command.
    h.keys("shift-tab shift-tab");
    h.press_space();
    assert_eq!(
        h.editor_kind(),
        Some(TunnelKind::Command),
        "Space on Command"
    );

    let mut reached = Vec::new();
    for _ in 0..8 {
        h.keys("tab");
        let field = h.focused_field();
        reached.push(field);
        if field == Some("Command") {
            h.keys("enter");
            assert!(h.dialog_open(), "Enter in the command doesn't save");
        }
        if field == Some("Startup timeout") {
            break;
        }
    }
    assert_eq!(
        reached.last().copied().flatten(),
        Some("Startup timeout"),
        "Tab reaches the timeout: {reached:?}"
    );
    assert!(reached.contains(&Some("Command")), "by way of the command");

    let (timeout, viewport) = h.drawn(TIMEOUT_FIELD_SELECTOR);
    assert!(
        timeout.top() >= viewport.top() && timeout.bottom() <= viewport.bottom(),
        "the timeout is scrolled into view: {timeout:?} in {viewport:?}"
    );
    let save = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find(SAVE_ID).bounds()
    });
    assert!(save.bottom() <= viewport.bottom(), "Save stays in view");
}

/// Escape closes the dialog with the tunnel as it was, focus back on its row.
#[gpui_kit::test]
async fn escape_keeps_the_original(cx: &mut TestAppContext) {
    let mut h = harness(cx, 600.);
    h.edit();
    h.vcx.simulate_input(" renamed");
    h.vcx.run_until_parked();

    h.keys("escape");
    assert!(!h.dialog_open(), "Escape closes the dialog");
    assert_eq!(h.saved_name(), "QA", "and keeps the tunnel as it was");
    assert!(h.row_focused(), "focus is back on the tunnel's row");
}

/// Enter in a single-line field saves and closes, focus back on the row.
#[gpui_kit::test]
async fn enter_saves(cx: &mut TestAppContext) {
    let mut h = harness(cx, 600.);
    h.edit();
    h.vcx.simulate_input(" 2");
    h.vcx.run_until_parked();

    h.keys("enter");
    assert!(!h.dialog_open(), "Enter saves and closes the dialog");
    assert_eq!(h.saved_name(), "QA 2");
    assert!(h.row_focused(), "focus is back on the tunnel's row");
}

/// A save that fails validation keeps the dialog open; Cancel then closes it.
#[gpui_kit::test]
async fn a_failed_save_stays_open(cx: &mut TestAppContext) {
    let mut h = harness(cx, 600.);
    h.click("tunnels-new");
    assert!(h.dialog_open(), "New Tunnel opens the editor as a dialog");
    assert_eq!(h.focused_field(), Some("Name"));

    h.keys("enter");
    assert!(h.dialog_open(), "an empty host and user don't save");

    h.click(CANCEL_ID);
    assert!(!h.dialog_open(), "Cancel closes it");
    assert_eq!(
        TunnelStore::new(h.path.clone()).list().len(),
        1,
        "nothing new was saved"
    );
}

/// The stored tunnel named `name`.
fn stored(path: &std::path::Path, name: &str) -> TunnelConfig {
    let config: crate::config::tunnels::TunnelsConfig = crate::config::load(path);
    config
        .tunnels
        .into_values()
        .find(|tunnel| tunnel.name == name)
        .unwrap_or_else(|| panic!("{name} is stored"))
}

/// `manual-confirmation-tunnels` 1.2: a new tunnel made manual from the keyboard -
/// the kind switch, the instruction and the reachability choice each reached with
/// Tab and pressed with Space - saves as a manual tunnel with those settings.
#[gpui_kit::test]
async fn a_manual_tunnel_is_created_from_the_keyboard(cx: &mut TestAppContext) {
    let mut h = harness(cx, 600.);
    h.click("tunnels-new");
    assert_eq!(h.focused_field(), Some("Name"));
    h.vcx.simulate_input("corp-vpn");
    // The kind switch sits just above Name: Manual is the stop before it.
    h.keys("shift-tab");
    h.press_space();
    assert_eq!(h.editor_kind(), Some(TunnelKind::Manual));
    let test_shown = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::ElementId::Name("tunnel-test".into()))
            .is_some()
    });
    assert!(!test_shown, "a manual tunnel has nothing to test");

    h.keys("tab tab");
    assert_eq!(h.focused_field(), Some("Instruction"));
    h.vcx
        .simulate_input("Connect the corporate VPN in the menu bar");
    // Skip the prompt, then Always prompt.
    h.keys("tab tab");
    h.press_space();
    // Back to the instruction, where Enter saves.
    h.keys("shift-tab shift-tab");
    assert_eq!(h.focused_field(), Some("Instruction"));
    h.keys("enter");
    assert!(!h.dialog_open(), "Enter saves and closes the dialog");

    let tunnel = stored(&h.path, "corp-vpn");
    assert_eq!(tunnel.kind, TunnelKind::Manual);
    assert_eq!(
        tunnel.manual.message.as_deref(),
        Some("Connect the corporate VPN in the menu bar")
    );
    assert!(!tunnel.manual.skip_when_reachable);
}

/// `manual-confirmation-tunnels` 1.2: an SSH tunnel switched to manual from the
/// keyboard saves as manual with the defaults, and keeps its SSH settings for a
/// switch back.
#[gpui_kit::test]
async fn an_ssh_tunnel_switches_to_manual_from_the_keyboard(cx: &mut TestAppContext) {
    let mut h = harness(cx, 600.);
    h.edit();
    h.keys("shift-tab");
    h.press_space();
    assert_eq!(h.editor_kind(), Some(TunnelKind::Manual));
    h.keys("tab");
    assert_eq!(h.focused_field(), Some("Name"));
    h.keys("enter");
    assert!(!h.dialog_open());

    let tunnel = TunnelStore::new(h.path.clone())
        .get(TUNNEL_ID)
        .expect("still stored under its id");
    assert_eq!(tunnel.kind, TunnelKind::Manual);
    assert_eq!(tunnel.manual.message, None);
    assert!(tunnel.manual.skip_when_reachable, "on by default");
    assert_eq!(tunnel.bastion_host, "bastion.example.com");
    assert_eq!(tunnel.bastion_user, "ops");
}
