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
