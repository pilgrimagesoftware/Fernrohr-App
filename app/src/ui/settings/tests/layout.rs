//! #139: the Settings window's layout - a conflict prompt stays inside the
//! window - and its Appearance section, reached and switched by keyboard.

use super::super::appearance::theme_button_id;
use super::super::{Section, SettingsWindow, ShowAppearance, ShowKeyboardShortcuts};
use super::{app, open, press};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext, VisualTestContext};

/// A conflict prompt's question runs long - `cmd-k` starts thirteen commands'
/// chords - but it wraps, so its Apply and Cancel buttons stay in the window.
#[gpui_kit::test]
async fn a_long_conflict_prompt_keeps_its_buttons_in_the_window(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, section) = open(main, cx);
    cx.update(|cx| section.update(cx, |s, cx| s.select("panel.focus_next", cx)));
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    press(&mut vcx, "enter");
    press(&mut vcx, "cmd-k");
    assert_eq!(cx.update(|cx| section.read(cx).test_mode()), "confirming");

    let (apply, cancel, width) = vcx
        .update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            let bounds = |id: &str| {
                window
                    .try_find(ElementId::Name(id.to_string().into()))
                    .unwrap_or_else(|| panic!("{id} is drawn"))
                    .bounds()
            };
            (
                bounds("shortcut-apply-panel.focus_next"),
                bounds("shortcut-cancel-panel.focus_next"),
                window.viewport_size().width,
            )
        })
        .unwrap();
    assert!(
        apply.right() <= width,
        "Apply inside: {apply:?}, width {width:?}"
    );
    assert!(
        cancel.right() <= width,
        "Cancel inside: {cancel:?}, width {width:?}"
    );
}

/// Runs a section command as the palette does: dispatched from the focus.
fn show(vcx: &mut VisualTestContext, action: Box<dyn gpui_kit::Action>) {
    vcx.update(|window, cx| window.dispatch_action(action, cx));
    vcx.run_until_parked();
}

fn space(vcx: &mut VisualTestContext) {
    let keystroke = gpui_kit::Keystroke::parse("space").expect("valid");
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    vcx.run_until_parked();
}

/// Tabs until the element `id` has focus, then presses Space on it.
fn press_button(vcx: &mut VisualTestContext, handle: gpui_kit::AnyWindowHandle, id: &str) {
    for _ in 0..40 {
        let focused = vcx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window
                    .try_find(ElementId::Name(id.to_string().into()))
                    .and_then(|element| element.focused())
            })
            .unwrap();
        if focused == Some(true) {
            space(vcx);
            return;
        }
        press(vcx, "tab");
    }
    panic!("Tab never reached {id}");
}

fn shown(
    cx: &mut TestAppContext,
    handle: gpui_kit::WindowHandle<gpui_kit::component::Root>,
) -> Section {
    handle
        .update(cx, |root, _window, cx| {
            root.view()
                .clone()
                .downcast::<SettingsWindow>()
                .expect("the Settings window's view")
                .read(cx)
                .section()
        })
        .unwrap()
}

fn drawn(
    vcx: &mut VisualTestContext,
    handle: gpui_kit::AnyWindowHandle,
    selector: &'static str,
) -> bool {
    let _ = vcx.update_window(handle, |_, window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector).is_some()
}

/// The sections switch by their commands and by the sidebar's buttons, from
/// the keyboard alone; Text Size is under Appearance, Shortcut Timeout under
/// Keyboard Shortcuts.
#[gpui_kit::test]
async fn sections_switch_by_key_and_by_sidebar_button(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, _) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    assert_eq!(shown(cx, handle), Section::KeyboardShortcuts);
    assert!(drawn(&mut vcx, handle.into(), "shortcut-timeout-value"));
    assert!(!drawn(&mut vcx, handle.into(), "text-size-value"));

    show(&mut vcx, Box::new(ShowAppearance));
    assert_eq!(shown(cx, handle), Section::Appearance);
    assert!(drawn(&mut vcx, handle.into(), "text-size-value"));
    assert!(!drawn(&mut vcx, handle.into(), "shortcut-timeout-value"));

    show(&mut vcx, Box::new(ShowKeyboardShortcuts));
    assert_eq!(shown(cx, handle), Section::KeyboardShortcuts);

    press_button(&mut vcx, handle.into(), Section::Appearance.button_id());
    assert_eq!(
        shown(cx, handle),
        Section::Appearance,
        "Tab and Space on it"
    );
}

/// Spec for #139: the theme selector applies the theme live.
#[gpui_kit::test]
async fn the_theme_buttons_change_the_theme(cx: &mut TestAppContext) {
    use crate::config::ui::Theme;
    let main = app(cx);
    let (handle, _) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowAppearance));
    press_button(&mut vcx, handle.into(), theme_button_id(Theme::Dark));
    assert_eq!(cx.update(|cx| crate::ui::theme::current(cx)), Theme::Dark);
    press_button(&mut vcx, handle.into(), theme_button_id(Theme::Light));
    assert_eq!(cx.update(|cx| crate::ui::theme::current(cx)), Theme::Light);
}
