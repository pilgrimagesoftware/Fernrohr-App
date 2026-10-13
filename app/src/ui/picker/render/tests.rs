// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
use crate::ui::picker::ClusterPicker;

/// Arrow keys move the selection, like a click does: keyboard navigation reaches
/// Connect and `context.set_tunnel` without the mouse.
#[gpui_kit::test]
async fn arrow_keys_move_the_selection(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::VisualTestContext;

    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, window, cx| {
            picker.contexts = Ok(vec!["kind-dev".to_string(), "staging".to_string()]);
            cx.notify();
            let focus = picker.command_focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();

    let selected = window
        .update(&mut vcx, |picker, _window, _cx| picker.selected_context())
        .unwrap();
    assert!(
        selected.is_some(),
        "a keyboard move selects a context, so Connect and the tunnel shortcut can act on it"
    );
}

/// Fernrohr-App#232: a long generated context name keeps its tunnel selector inside
/// the card, at the narrowest window the picker allows and at a wide one, where the
/// card grows to give the name more room.
#[gpui_kit::test]
async fn a_long_name_keeps_its_tunnel_selector_inside_the_card(cx: &mut gpui_kit::TestAppContext) {
    use crate::ui::picker::{CARD_SELECTOR, MIN_WINDOW_SIZE, row_tunnel_selector};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{VisualTestContext, px, size};

    // Placeholder project and cluster, shaped like gcloud's generated names.
    let long = "gke_example-project-0000_us-central1_example-cluster-with-a-long-name";
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, cx| {
            picker.contexts = Ok(vec![long.to_string(), "kind-dev".to_string()]);
            cx.notify();
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let selector: &'static str = row_tunnel_selector(long).leak();

    let mut widths = Vec::new();
    for width in [MIN_WINDOW_SIZE.width, px(1200.)] {
        vcx.simulate_resize(size(width, MIN_WINDOW_SIZE.height));
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        let card = vcx.debug_bounds(CARD_SELECTOR).expect("the card is drawn");
        let tunnel = vcx
            .debug_bounds(selector)
            .expect("the long row's tunnel selector is drawn");
        assert!(
            tunnel.left() >= card.left() && tunnel.right() <= card.right(),
            "at a {width:?} window the tunnel selector {tunnel:?} is outside the card {card:?}"
        );
        assert!(
            card.right() <= width,
            "the card fits the {width:?} window: {card:?}"
        );
        widths.push(card.size.width);
    }
    assert!(
        widths[1] > widths[0],
        "the card grows with the window: {widths:?}"
    );
}
