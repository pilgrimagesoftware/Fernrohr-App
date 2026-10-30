//! #309 through a drawn panel: a line with ANSI colours is drawn styled, and
//! selecting and copying it gives its plain text, with no escape in it.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px,
};

/// A Logs panel showing `lines` as a pod's, drawn.
fn drawn(lines: &[&str], cx: &mut TestAppContext) -> (Entity<LogsPanel>, VisualTestContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built: Option<Entity<LogsPanel>> = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            panel.test_show_lines("web-1", "web", lines, cx);
            panel
        });
        built = Some(panel);
        Root::new(built.clone().expect("built"), window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    (built.expect("the window built its panel"), vcx)
}

#[gpui_kit::test]
async fn a_coloured_line_is_drawn_styled_and_copies_as_plain_text(cx: &mut TestAppContext) {
    let (panel, mut vcx) = drawn(
        &[
            "\x1b[1;31mERROR\x1b[0m boom \x1b]0;title\x07\x1b[2K",
            "\x1b[38;5;208mwarn\x1b[0m slow",
        ],
        cx,
    );
    assert!(
        vcx.debug_bounds(super::super::super::render::STYLED_LINE)
            .is_some(),
        "the coloured line is drawn with its styling"
    );
    let texts: Vec<String> = vcx.update(|_, cx| {
        panel
            .read(cx)
            .view
            .read(cx)
            .lines()
            .iter()
            .map(ToString::to_string)
            .collect()
    });
    assert_eq!(
        texts,
        ["ERROR boom ", "warn slow"],
        "the lines' text is plain"
    );

    let list = vcx
        .debug_bounds(super::super::super::render::LINES)
        .expect("the lines are drawn");
    let inset = vcx.update(|_, cx| crate::ui::space::spacing(cx).panel_inset);
    let start = point(list.left() + inset + px(1.), list.top() + inset + px(4.));
    let end = point(list.right() - px(1.), list.bottom() - px(1.));
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let _ = window.draw(cx);
        let focus = panel.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    vcx.simulate_keystrokes("secondary-c");
    vcx.run_until_parked();
    let copied = vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap_or_default();
    assert!(
        copied.contains("ERROR boom") && copied.contains("warn slow"),
        "both lines copied: {copied:?}"
    );
    assert!(
        !copied.contains('\x1b'),
        "with no escape in them: {copied:?}"
    );
}

#[gpui_kit::test]
async fn a_plain_line_draws_as_before(cx: &mut TestAppContext) {
    let (_panel, mut vcx) = drawn(&["level=info msg=ready"], cx);
    assert!(
        vcx.debug_bounds(super::super::super::render::STYLED_LINE)
            .is_none(),
        "no styled copy for a plain line"
    );
}
