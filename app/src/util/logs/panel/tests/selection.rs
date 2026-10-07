//! #151: the Logs panel's lines can be drag-selected - across several lines,
//! in order - so their text can be copied.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px,
};

#[gpui_kit::test]
async fn dragging_across_log_lines_selects_their_text(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built: Option<gpui_kit::Entity<LogsPanel>> = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            panel.test_show_lines(
                "web-1",
                "web",
                &["first line", "second line", "third line"],
                cx,
            );
            panel
        });
        built = Some(panel);
        Root::new(built.clone().expect("built"), window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    // `draw`, not `render_frame`: the selection layer reads the hitboxes the
    // drawn frame left, which the mouse events below hit-test against.
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let list = vcx
        .debug_bounds(super::super::super::render::LINES)
        .expect("the lines are drawn");

    // From the start of the first line to past the end of the third.
    let inset = vcx.update(|_, cx| crate::ui::space::spacing(cx).panel_inset);
    let start = point(list.left() + inset + px(1.), list.top() + inset + px(4.));
    let end = point(list.right() - px(1.), list.bottom() - px(1.));
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();

    let selected = vcx.update(|window, cx| {
        let _ = window.draw(cx);
        gpui_kit::base::TextSelection::selected_text(window, cx)
    });
    let first = selected.find("first line");
    let third = selected.find("third line");
    assert!(
        first.is_some() && third.is_some() && first < third && selected.contains("second line"),
        "the drag selected all three lines, in order: {selected:?}"
    );

    // And the copy key copies them, with focus in the panel.
    let panel = built.expect("the window built its panel");
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let focus = panel.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    vcx.simulate_keystrokes("secondary-c");
    vcx.run_until_parked();
    let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(
        copied.as_deref(),
        Some(selected.as_str()),
        "copied what's selected"
    );
}
