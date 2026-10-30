//! #236: the background MCP preference defaults to keep running, and each
//! command sets and saves it.

use super::{BackgroundMcp, KeepMcpInBackground, StopMcpWithLastWindow, current, init};
use crate::config::{self, ui::UiConfig};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn it_defaults_to_keep_running_and_each_command_sets_and_saves_it(cx: &mut TestAppContext) {
    assert_eq!(
        UiConfig::default().background_mcp,
        BackgroundMcp::KeepRunning
    );
    let path = crate::util::test_paths::temp_path("background-mcp-ui");
    let _ = std::fs::remove_file(&path);
    cx.update(|cx| {
        init(BackgroundMcp::KeepRunning, path.clone(), cx);
        super::register_handlers(cx);
    });
    cx.update(|cx| cx.dispatch_action(&StopMcpWithLastWindow));
    assert_eq!(
        cx.update(|cx| current(cx)),
        BackgroundMcp::StopWithLastWindow
    );
    let saved: UiConfig = config::load(&path);
    assert_eq!(saved.background_mcp, BackgroundMcp::StopWithLastWindow);
    cx.update(|cx| cx.dispatch_action(&KeepMcpInBackground));
    assert_eq!(cx.update(|cx| current(cx)), BackgroundMcp::KeepRunning);
    let _ = std::fs::remove_file(&path);
}
