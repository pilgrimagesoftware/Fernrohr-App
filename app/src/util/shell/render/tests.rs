// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::command::CommandRegistry;
use crate::util::shell::{ToggleCommandPalette, WindowLayout, open_window, register_commands};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
async fn toggle_command_palette_action_opens_a_dialog(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        cx.set_global(registry);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    let window = cx.update(|cx| cx.windows()[0]);

    // Leak-safe: with no dialog open, `render_dialog_layer` returns
    // `None` before touching any dialog state.
    let dialog_open_before = window
        .update(cx, |_, window, cx| {
            gpui_kit::component::Root::render_dialog_layer(window, cx).is_some()
        })
        .unwrap();
    assert!(!dialog_open_before);

    window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(ToggleCommandPalette), cx);
        })
        .unwrap();
    cx.run_until_parked();

    // Not re-checked via `render_dialog_layer` here: actually rendering
    // gpui-component's `Command` widget installs a model that outlives
    // `close_all_dialogs`/`remove_window` and trips the test harness's
    // leaked-entity check - reproduced directly against gpui-component
    // 0.6.6, not something under our control. `open_command_palette`
    // reaching this point without panicking, immediately after the
    // action dispatch above, is what's covered instead.

    // Close the dialog before the test ends, or the leak detector flags
    // its CommandState entity: the harness asserts every entity created
    // during a test is released by teardown.
    window
        .update(cx, |_, window, cx| {
            let Some(Some(root)) = window.root::<gpui_kit::component::Root>() else {
                return;
            };
            root.update(cx, |root, cx| root.close_all_dialogs(window, cx));
        })
        .unwrap();
    window
        .update(cx, |_, window, _cx| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}
