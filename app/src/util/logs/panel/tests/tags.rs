//! `selector-logs-panel` 3.2: a tagged line draws its tag - in its pod's
//! colour - before its text; a single pod's lines have no tag.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::logs::LogEvent;
use crate::util::logs::line::LineSource;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};

const LINE_TAG: &str = super::super::super::render::LINE_TAG;

#[gpui_kit::test]
async fn a_tagged_line_draws_its_tag_and_a_plain_one_does_not(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            panel.test_show_lines("web-1", "app", &["plain"], cx);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("built");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let draw = |vcx: &mut VisualTestContext| {
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    };
    draw(&mut vcx);
    assert!(
        vcx.debug_bounds(LINE_TAG).is_none(),
        "a plain line, untagged"
    );

    vcx.update(|_, cx| {
        let view = panel.read(cx).view.clone();
        view.update(cx, |view, cx| {
            view.apply_from(
                &LineSource::new("web-2/sidecar", "web-2"),
                LogEvent::Line("tagged".into()),
            );
            cx.notify();
        });
    });
    draw(&mut vcx);
    let tag = vcx.debug_bounds(LINE_TAG).expect("the tag is drawn");
    let lines = vcx
        .debug_bounds(super::super::super::render::LINES)
        .expect("the lines are drawn");
    assert!(lines.contains(&tag.origin), "in the list, before its text");
}

/// Every pod colour reads as text on the background, in both bundled modes.
#[gpui_kit::test]
fn every_pod_colour_reads_in_both_modes(cx: &mut TestAppContext) {
    use crate::ui::style::{TEXT_CONTRAST, contrast};
    use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode};
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            Theme::change(mode, None, cx);
            crate::ui::accent::refresh(cx);
            let background = cx.theme().background;
            for colour in super::super::super::render::pod_tag_colours(cx) {
                let ratio = contrast(colour, background);
                assert!(ratio >= TEXT_CONTRAST, "{mode:?}: {ratio:.2}:1");
            }
        });
    }
}

/// #238 review (#151): copying a tagged line copies its tag too - the
/// coloured tag is drawn over the selectable `tag │ text`, not instead of it.
#[gpui_kit::test]
async fn copying_a_tagged_line_keeps_its_tag(cx: &mut TestAppContext) {
    use gpui_kit::{Modifiers, MouseButton, point, px};
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".into()), cx);
            panel.test_show_lines("web-1", "app", &[], cx);
            let view = panel.view.clone();
            view.update(cx, |view, _| {
                let source = LineSource::new("web-1", "web-1");
                view.apply_from(&source, LogEvent::Line("first".into()));
                let source = LineSource::new("web-2", "web-2");
                view.apply_from(&source, LogEvent::Line("second".into()));
            });
            panel
        });
        Root::new(panel, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
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
    let selected = vcx.update(|window, cx| {
        let _ = window.draw(cx);
        gpui_kit::base::TextSelection::selected_text(window, cx)
    });
    assert!(
        selected.contains("web-1 \u{2502} first") && selected.contains("web-2 \u{2502} second"),
        "each line copies with its pod: {selected:?}"
    );
}
