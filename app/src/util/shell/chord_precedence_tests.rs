//! #137: a key that is a command on its own and also starts the dock's
//! chords waits for the Shortcut timeout before it runs. The user's case is
//! Focus Resources rebound to `cmd-k`, either in `keymap.toml` at launch or
//! live in Settings. GPUI keeps a chord pending against a complete shorter
//! binding only when the chord was bound after it, so both paths now bind a
//! chord after any shorter key it starts with.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A window on `demo` showing Pods, focused, with `keymap_text` as
/// `keymap.toml`.
fn harness(cx: &mut TestAppContext, keymap_text: &str) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    if !keymap_text.is_empty() {
        std::fs::write(&keymap, keymap_text).unwrap();
    }
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    vcx.simulate_keystrokes("cmd-1");
    vcx.run_until_parked();
    Harness { main, vcx }
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn wait(&mut self, millis: u64) {
        self.vcx
            .executor()
            .advance_clock(Duration::from_millis(millis));
        self.vcx.run_until_parked();
    }

    fn resources_focused(&mut self) -> bool {
        let main = self.main.clone();
        self.vcx.update(|window, cx| {
            main.read(cx)
                .test_resource_panel()
                .expect("a workspace window")
                .read(cx)
                .focus_handle()
                .contains_focused(window, cx)
        })
    }

    fn open_panels(&mut self) -> usize {
        let main = self.main.clone();
        self.vcx.update(|_, cx| match &main.read(cx).mode {
            crate::util::shell::WindowMode::Workspace { open_panels, .. } => open_panels.len(),
            crate::util::shell::WindowMode::Picker(_) => 0,
        })
    }

    fn pending(&mut self) -> bool {
        self.vcx
            .update(|window, _| window.pending_input().is_some())
    }

    /// `cmd-k` waits about 3 s, showing it's pending, then runs Focus
    /// Resources.
    fn assert_cmd_k_waits_then_focuses_resources(&mut self) {
        assert!(!self.resources_focused());
        self.press("cmd-k");
        assert!(self.pending(), "cmd-k is pending, not run at once");
        assert!(!self.resources_focused());
        self.wait(2_900);
        assert!(!self.resources_focused(), "still waiting at 2.9 s");
        self.wait(200);
        assert!(!self.pending());
        assert!(self.resources_focused(), "Focus Resources ran by 3.1 s");
    }
}

#[gpui_kit::test]
async fn a_rebinding_in_the_keymap_file_waits_the_shortcut_timeout(cx: &mut TestAppContext) {
    let mut h = harness(cx, "[bindings]\n\"resource.focus\" = \"cmd-k\"\n");
    h.assert_cmd_k_waits_then_focuses_resources();
}

#[gpui_kit::test]
async fn a_rebinding_made_live_waits_the_shortcut_timeout(cx: &mut TestAppContext) {
    let mut h = harness(cx, "");
    h.vcx
        .update(|_, cx| {
            crate::keymap::apply(
                cx,
                "resource.focus",
                crate::keymap::Edit::Set("cmd-k".into()),
            )
        })
        .expect("saved");
    h.vcx.run_until_parked();
    h.assert_cmd_k_waits_then_focuses_resources();
}

/// The chords still complete while the shorter key waits: `cmd-k left`
/// splits, and Focus Resources never runs.
#[gpui_kit::test]
async fn the_chords_still_complete_under_a_rebound_prefix(cx: &mut TestAppContext) {
    let mut h = harness(cx, "[bindings]\n\"resource.focus\" = \"cmd-k\"\n");
    assert_eq!(h.open_panels(), 1);
    h.press("cmd-k left");
    assert!(!h.pending());
    assert_eq!(h.open_panels(), 2, "the split ran");
    h.wait(5_000);
    assert!(!h.resources_focused(), "Focus Resources never ran");
}
