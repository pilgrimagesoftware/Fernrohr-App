//! `resource-detail-ui-improvements` 3, in the object panel: Copy Resource
//! Name (`c`), and the copy controls on a ConfigMap's keys and values - by
//! click, and by Tab and Space.

use super::fixtures::{kind, object, stub_panel, target};
use crate::command::CommandRegistry;
use crate::keymap::{self, KeymapConfig};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Modifiers, TestAppContext, VisualTestContext, WindowHandle,
};
use serde_json::json;

fn config_map_panel(cx: &mut TestAppContext) -> (WindowHandle<Root>, VisualTestContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let config_maps = kind("", "v1", "ConfigMap", true);
    let (window, panel) = stub_panel(
        cx,
        target(config_maps.clone(), Some("staging"), "app-config"),
        vec![config_maps],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(
                    object(json!({
                        "apiVersion": "v1", "kind": "ConfigMap",
                        "metadata": { "name": "app-config", "namespace": "staging" },
                        "data": { "LOG_LEVEL": "debug" },
                    })),
                    cx,
                )
            });
            panel.read(cx).focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    (window, vcx)
}

fn clipboard(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn click(window: WindowHandle<Root>, vcx: &mut VisualTestContext, id: &str) {
    let center = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(ElementId::Name(id.to_string().into()))
                .unwrap_or_else(|| panic!("{id} is drawn"))
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(center, Modifiers::none());
    vcx.run_until_parked();
}

/// Spec: "Copying the name" - `c` puts the object's name on the clipboard.
#[gpui_kit::test]
async fn c_copies_the_resource_name(cx: &mut TestAppContext) {
    let (_window, mut vcx) = config_map_panel(cx);
    vcx.simulate_keystrokes("c");
    vcx.run_until_parked();
    assert_eq!(clipboard(&mut vcx).as_deref(), Some("app-config"));
}

/// A ConfigMap's key and value each copy from their control.
#[gpui_kit::test]
async fn a_config_map_key_and_value_copy_on_click(cx: &mut TestAppContext) {
    let (window, mut vcx) = config_map_panel(cx);
    click(window, &mut vcx, "copy ConfigMap/Data value 0");
    assert_eq!(clipboard(&mut vcx).as_deref(), Some("debug"));
    click(window, &mut vcx, "copy ConfigMap/Data key 0");
    assert_eq!(clipboard(&mut vcx).as_deref(), Some("LOG_LEVEL"));
}

/// The keyboard reaches a copy control: Tab to the value's, then Space.
#[gpui_kit::test]
async fn a_copy_control_is_reached_and_pressed_by_keyboard(cx: &mut TestAppContext) {
    let (window, mut vcx) = config_map_panel(cx);
    let id = ElementId::Name("copy ConfigMap/Data value 0".into());
    let mut focused = false;
    for _ in 0..20 {
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
        focused = vcx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window
                    .try_find(id.clone())
                    .and_then(|button| button.focused())
                    == Some(true)
            })
            .unwrap();
        if focused {
            break;
        }
    }
    assert!(focused, "Tab reaches the value's copy control");
    let space = gpui_kit::Keystroke::parse("space").unwrap();
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    vcx.run_until_parked();
    assert_eq!(clipboard(&mut vcx).as_deref(), Some("debug"));
}

/// #187: a copy control shows it copied - a check mark and "Copied" - for a
/// moment after a click, then returns to its copy icon.
#[gpui_kit::test]
async fn a_copy_control_shows_it_copied_for_a_moment(cx: &mut TestAppContext) {
    use crate::consts::COPIED_FEEDBACK;
    use crate::ui::copy::{copied_selector, tooltip_text};
    let (window, mut vcx) = config_map_panel(cx);
    let id = ElementId::Name("copy ConfigMap/Data value 0".into());
    let shown = |vcx: &mut VisualTestContext| {
        vcx.run_until_parked();
        vcx.debug_bounds(copied_selector(&id).leak()).is_some()
    };
    assert!(!shown(&mut vcx), "nothing copied yet");

    click(window, &mut vcx, "copy ConfigMap/Data value 0");

    assert!(shown(&mut vcx), "the control shows it copied");
    vcx.executor().advance_clock(COPIED_FEEDBACK);
    assert!(!shown(&mut vcx), "then goes back");
    assert_eq!(tooltip_text(true), "Copied");
    assert_eq!(tooltip_text(false), "Copy");
}
