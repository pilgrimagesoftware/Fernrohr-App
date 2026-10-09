//! The navigation tools' fixtures: workspace windows on connected sessions
//! (section 2's `read::test_support`, against a recording fake API server
//! with the core kinds as discovery) and saved layouts with real dock dumps.
//! Used by `navigate`'s own tests and by `mcp::integration`'s, which drive
//! the same windows through the adapter.
//!
//! Named imports rather than a `gpui_kit::*` glob: see
//! `util/shell/open/tests.rs` on the macro-expansion budget.

use crate::config::saved_layouts::SavedLayout;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::test_recorder::Recorder;
use crate::mcp::read::test_support::{open, tools};
use crate::mcp::tools::ToolContext;
use crate::ui::nav::NavTarget;
use crate::util::shell::MainWindow;
use crate::util::test_paths::temp_path;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, WindowHandle};
use serde_json::json;
use std::path::PathBuf;

pub(in crate::mcp) struct Harness {
    pub(in crate::mcp) tools: ToolContext,
    pub(in crate::mcp) layouts_dir: PathBuf,
    /// The fake API server the sessions' client talks to, kept running.
    _api: Recorder,
    pub(in crate::mcp) client: kube::Client,
}

/// The app's real `init` with a temp workspace and layouts directory, a
/// kubeconfig listing `known`, and a connected session for each of `open`
/// whose discovery holds only the core kinds.
pub(in crate::mcp) fn harness_with(
    cx: &mut TestAppContext,
    known: &[&str],
    opened: &[&str],
) -> Harness {
    let tools = tools(cx, known);
    let layouts_dir = temp_path("mcp-layouts");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::util::shell::init(cx, temp_path("mcp-shell"), &temp_path("mcp-keymap"));
        crate::util::shell::set_layouts_dir_for_test(layouts_dir.clone(), cx);
    });
    let runtime = cx.update(|cx| crate::runtime::handle(cx));
    let (api, client) = Recorder::start(&runtime, "200 OK", json!({}));
    let h = Harness {
        tools,
        layouts_dir,
        _api: api,
        client,
    };
    reopen(cx, &h, opened);
    h
}

/// Opens a connected session on each of `contexts`, discovering the core
/// kinds - again, for a context whose last window closed and took its session
/// with it.
pub(in crate::mcp) fn reopen(cx: &mut TestAppContext, h: &Harness, contexts: &[&str]) {
    for context in contexts {
        open(
            cx,
            context,
            ConnectionState::Connected(h.client.clone()),
            Some(core_kinds()),
        );
    }
}

pub(in crate::mcp) fn core_kinds() -> Vec<DiscoveredKind> {
    vec![DiscoveredKind::pods(), DiscoveredKind::events()]
}

/// [`harness_with`] where every known context is open.
pub(in crate::mcp) fn harness(cx: &mut TestAppContext, contexts: &[&str]) -> Harness {
    harness_with(cx, contexts, contexts)
}

/// A workspace window holding `contexts`.
pub(in crate::mcp) fn workspace(
    cx: &mut TestAppContext,
    contexts: &[&str],
) -> (WindowHandle<Root>, Entity<MainWindow>) {
    let contexts: Vec<String> = contexts.iter().map(|c| c.to_string()).collect();
    let held = contexts.clone();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(contexts, window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    cx.run_until_parked();
    // The window's Resource panel ran discovery against the fake API server,
    // which serves no kinds; put the fixture's back.
    cx.update(|cx| {
        for context in &held {
            DiscoveryRegistry::insert_test(cx, context, core_kinds());
        }
    });
    (window, built.expect("the window built its MainWindow"))
}

pub(in crate::mcp) fn open_targets(
    cx: &mut TestAppContext,
    main: &Entity<MainWindow>,
) -> Vec<NavTarget> {
    cx.update(|cx| main.read(cx).test_open_targets())
}

/// A saved layout dumped from a throwaway window holding `contexts`, with
/// an Events list opened for each of `events_in` - then closed, so it holds
/// nothing by the time the layout loads.
pub(in crate::mcp) fn saved_layout(
    cx: &mut TestAppContext,
    name: &str,
    contexts: &[&str],
    events_in: &[&str],
) -> SavedLayout {
    let (window, main) = workspace(cx, contexts);
    for context in events_in {
        let context = context.to_string();
        window
            .update(cx, |_, window, cx| {
                main.update(cx, |main, cx| {
                    main.open_target_in(
                        NavTarget::Kind(DiscoveredKind::events()),
                        None,
                        Some(context),
                        Vec::new(),
                        crate::ui::nav::OpenMode::Foreground,
                        window,
                        cx,
                    )
                })
            })
            .unwrap();
    }
    cx.run_until_parked();
    let dock = cx.update(|cx| main.read(cx).test_dock_dump(cx));
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: name.into(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: contexts.iter().map(|c| c.to_string()).collect(),
        dock,
        resource_panel_width: Some(300.0),
        window_width: 900.0,
        window_height: 600.0,
    }
}
