//! The test app: the real command registry and keymap, sessions seeded with
//! fixed states (no cluster is ever reached), agents, workspace windows, and the
//! Connections window opened by its command key from a stand-in window.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{AgentId, AgentLabels, ClusterRegistry};
use crate::ui::connections::rows::ConnectionRow;
use crate::ui::connections::window::ConnectionsWindow;
use crate::util::shell::MainWindow;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, Render, TestAppContext,
    VisualTestContext, Window, WindowHandle, div,
};

/// Initialises the app: UI, runtime, and every command bound as in production.
pub(super) fn init(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(
            cx,
            crate::util::test_paths::temp_path("connections-workspace"),
            &crate::util::test_paths::temp_path("connections-keymap"),
        );
    });
}

/// Seeds `context`'s session in `state`, held by nobody yet.
pub(super) fn session(cx: &mut TestAppContext, context: &str, state: ConnectionState) {
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, context, state);
    });
}

/// A new agent holding `context`, reporting `name` (or none).
pub(super) fn agent(cx: &mut TestAppContext, context: &str, name: Option<&str>) -> AgentId {
    let agent = AgentId::next();
    cx.update(|cx| {
        AgentLabels::set(cx, agent, name.map(str::to_string));
        ClusterRegistry::hold(cx, context, agent);
    });
    agent
}

/// A workspace window holding `contexts` (their sessions seeded already).
pub(super) fn workspace(
    cx: &mut TestAppContext,
    contexts: &[&str],
) -> (WindowHandle<Root>, Entity<MainWindow>) {
    let contexts: Vec<String> = contexts.iter().map(|c| c.to_string()).collect();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(contexts, window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    cx.run_until_parked();
    (window, built.expect("the window built its MainWindow"))
}

struct Stand;

impl Render for Stand {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The open Connections window: its test context, and its view for reads (the
/// view can't be reached through the window's own context while that is
/// updating).
pub(super) struct Win {
    pub(super) vcx: VisualTestContext,
    view: Entity<ConnectionsWindow>,
}

/// The Connections window opened by its command's default key, pressed in a
/// stand-in window.
pub(super) fn open(cx: &mut TestAppContext) -> Win {
    let stand = cx.add_window(|_, _| Stand);
    let mut from = VisualTestContext::from_window(stand.into(), cx);
    from.update(|window, _| window.activate_window());
    press(&mut from, super::super::commands::OPEN_DEFAULT_BINDING);
    let handle = cx
        .update(|cx| crate::ui::connections::window::test_handle(cx))
        .expect("the Connections window is open");
    let view = cx
        .update(crate::ui::connections::window::test_view)
        .expect("its view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    Win { vcx, view }
}

/// Presses `keys` and lets the app settle.
pub(super) fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    settle(vcx);
}

pub(super) fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..10 {
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

impl Win {
    pub(super) fn press(&mut self, keys: &str) {
        press(&mut self.vcx, keys);
    }

    pub(super) fn settle(&mut self) {
        settle(&mut self.vcx);
    }

    /// The rows the window lists now.
    pub(super) fn rows(&mut self) -> Vec<ConnectionRow> {
        self.view.read_with(&self.vcx, |view, cx| view.rows(cx))
    }

    /// The contexts the window lists now, in order.
    pub(super) fn listed(&mut self) -> Vec<String> {
        self.rows().into_iter().map(|row| row.context).collect()
    }

    /// The selected context.
    pub(super) fn selected(&mut self) -> Option<String> {
        self.view
            .read_with(&self.vcx, |view, _| view.selected.clone())
    }

    pub(super) fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    /// Whether `selector`'s element was drawn in the last frame.
    pub(super) fn drawn(&mut self, selector: &str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx
            .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
            .is_some()
    }

    /// Clicks the element `id` (a button).
    pub(super) fn click(&mut self, id: &str) {
        let id: &'static str = Box::leak(id.to_string().into_boxed_str());
        let at = self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        });
        self.click_at(at);
    }

    /// Clicks the element drawn with debug selector `selector`.
    pub(super) fn click_selector(&mut self, selector: &str) {
        self.vcx.update(|window, cx| window.render_frame(cx));
        let at = self
            .vcx
            .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
            .expect("drawn")
            .center();
        self.click_at(at);
    }

    fn click_at(&mut self, at: gpui_kit::Point<gpui_kit::Pixels>) {
        self.vcx.simulate_click(at, Modifiers::none());
        self.settle();
    }
}

/// How many holds on `context` are windows.
pub(super) fn window_holders(cx: &mut TestAppContext, context: &str) -> usize {
    cx.update(|cx| ClusterRegistry::window_holder_count(cx, context))
}
