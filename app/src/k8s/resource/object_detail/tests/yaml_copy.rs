//! #154: the YAML view's lines can be drag-selected - several, in order - and
//! copied with the copy key; Copy YAML (`shift-c`, or the view's button) puts
//! the whole manifest on the clipboard, folded blocks included.

use super::yaml::yaml_panel;
use crate::ui::yaml_view::{COPY_YAML_BUTTON, line_selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Modifiers, MouseButton, TestAppContext, VisualTestContext, point,
    px,
};

fn clipboard(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui_kit::test]
async fn dragging_across_yaml_lines_selects_them_and_the_copy_key_copies(cx: &mut TestAppContext) {
    let (_window, panel, mut vcx) = yaml_panel(cx);
    // `draw`, not `render_frame`: the selection layer reads the hitboxes the
    // drawn frame left, which the mouse events below hit-test against.
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let first = vcx
        .debug_bounds(line_selector(0).leak())
        .expect("line 0 is drawn");
    let third = vcx
        .debug_bounds(line_selector(2).leak())
        .expect("line 2 is drawn");

    let start = point(first.left() + px(1.), first.center().y);
    let end = point(third.right() - px(1.), third.center().y);
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();

    let selected = vcx.update(|window, cx| {
        let _ = window.draw(cx);
        gpui_kit::base::TextSelection::selected_text(window, cx)
    });
    let yaml = vcx.update(|_, cx| panel.read(cx).yaml().expect("loaded"));
    let lines: Vec<&str> = yaml.lines().take(3).collect();
    let at: Vec<Option<usize>> = lines
        .iter()
        .map(|line| selected.find(line.trim_end()))
        .collect();
    assert!(
        at.iter().all(Option::is_some) && at.windows(2).all(|pair| pair[0] < pair[1]),
        "the drag selected the first three lines, in order: {selected:?}"
    );

    vcx.simulate_keystrokes("secondary-c");
    vcx.run_until_parked();
    assert_eq!(
        clipboard(&mut vcx),
        Some(selected),
        "copied what's selected"
    );
}

#[gpui_kit::test]
async fn shift_c_copies_the_whole_manifest_folded_or_not(cx: &mut TestAppContext) {
    let (_window, panel, mut vcx) = yaml_panel(cx);
    let yaml = vcx.update(|_, cx| panel.read(cx).yaml().expect("loaded"));

    vcx.simulate_keystrokes("z shift-c");
    vcx.run_until_parked();
    assert_eq!(clipboard(&mut vcx), Some(yaml));
}

#[gpui_kit::test]
async fn the_views_copy_button_copies_the_manifest(cx: &mut TestAppContext) {
    let (window, panel, mut vcx) = yaml_panel(cx);
    let yaml = vcx.update(|_, cx| panel.read(cx).yaml().expect("loaded"));
    let button = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(ElementId::Name(COPY_YAML_BUTTON.into()))
                .expect("the view has a Copy YAML button")
                .bounds()
                .center()
        })
        .unwrap();

    vcx.simulate_click(button, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(clipboard(&mut vcx), Some(yaml));
}
