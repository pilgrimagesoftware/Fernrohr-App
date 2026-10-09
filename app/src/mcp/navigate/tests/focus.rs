//! `mcp-connect-and-focus` 1.1 (design D4): a tool that changes what a window
//! shows brings Fernrohr forward with that window focused; a tool that only
//! reads leaves focus where it was. A non-workspace window stands in for the
//! agent's terminal: active before the call, so a change of active window is
//! the tool's doing.

use super::*;
use gpui_kit::{AnyWindowHandle, Context, IntoElement, Render, Window, div};

struct Terminal;

impl Render for Terminal {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Opens the stand-in terminal and makes it the active window.
fn terminal(cx: &mut TestAppContext) -> AnyWindowHandle {
    let window: AnyWindowHandle = cx.add_window(|_, _| Terminal).into();
    cx.update(|cx| {
        window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap()
    });
    cx.run_until_parked();
    assert_eq!(active(cx), Some(window.window_id()));
    window
}

fn active(cx: &mut TestAppContext) -> Option<gpui_kit::WindowId> {
    cx.update(|cx| cx.active_window()).map(|w| w.window_id())
}

#[gpui_kit::test]
async fn open_panel_brings_its_window_forward(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev", "prod"]);
    let (_dev, _) = workspace(cx, &["dev"]);
    let (prod, _) = workspace(cx, &["prod"]);
    terminal(cx);

    content(
        call(
            cx,
            &h,
            "open_panel",
            json!({"context": "prod", "kind": "Pod"}),
        )
        .await,
    );

    assert_eq!(
        active(cx),
        Some(prod.window_id()),
        "the window holding prod"
    );
}

#[gpui_kit::test]
async fn load_layout_brings_its_window_forward(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let layout = saved_layout(cx, "Triage", &["dev"], &[]);
    saved_layouts::save(&h.layouts_dir, &layout).expect("seeds a layout");
    reopen(cx, &h, &["dev"]);
    let (dev, _) = workspace(cx, &["dev"]);
    terminal(cx);

    content(
        call(
            cx,
            &h,
            "load_layout",
            json!({"name": "Triage", "mode": "add"}),
        )
        .await,
    );

    assert_eq!(active(cx), Some(dev.window_id()));
}

#[gpui_kit::test]
async fn reads_and_list_layouts_leave_focus_alone(cx: &mut TestAppContext) {
    let h = harness(cx, &["dev"]);
    let _ = workspace(cx, &["dev"]);
    let terminal = terminal(cx);

    content(call(cx, &h, "list_layouts", json!({})).await);
    content(call(cx, &h, "list_contexts", json!({})).await);
    content(call(cx, &h, "list_resource_kinds", json!({"context": "dev"})).await);

    assert_eq!(active(cx), Some(terminal.window_id()), "still the terminal");
}
