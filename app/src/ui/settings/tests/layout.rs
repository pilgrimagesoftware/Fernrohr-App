//! #139: the Settings window's layout - a conflict prompt stays inside the
//! window - and its Appearance section, reached and switched by keyboard.

use super::super::appearance::theme_button_id;
use super::super::panels::logs_button_id;
use super::super::{Section, SettingsWindow, ShowAppearance, ShowKeyboardShortcuts, ShowPanels};
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
///
/// `pub(super)`: shared with the sibling `saved_layouts` test module, not
/// just this file's own tests.
pub(super) fn show(vcx: &mut VisualTestContext, action: Box<dyn gpui_kit::Action>) {
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
///
/// `pub(super)`: shared with the sibling `saved_layouts` test module.
pub(super) fn press_button(
    vcx: &mut VisualTestContext,
    handle: gpui_kit::AnyWindowHandle,
    id: &str,
) {
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

/// `pub(super)`: shared with the sibling `saved_layouts` test module.
pub(super) fn shown(
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

/// `pub(super)`: shared with the sibling `saved_layouts` test module.
pub(super) fn drawn(
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

/// `logs-panel-instancing`: the Panels section is reached by its command and
/// its sidebar button, and its buttons - from the keyboard - set where pod
/// logs open, live.
#[gpui_kit::test]
async fn the_panels_section_sets_where_pod_logs_open(cx: &mut TestAppContext) {
    use crate::config::ui::LogsPanels;
    let main = app(cx);
    let (handle, _) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    show(&mut vcx, Box::new(ShowPanels));
    assert_eq!(shown(cx, handle), Section::Panels);
    show(&mut vcx, Box::new(ShowKeyboardShortcuts));
    press_button(&mut vcx, handle.into(), Section::Panels.button_id());
    assert_eq!(shown(cx, handle), Section::Panels, "Tab and Space on it");

    assert_eq!(
        cx.update(|cx| crate::ui::logs_panels::current(cx)),
        LogsPanels::PerPod,
        "each pod's own panel by default"
    );
    press_button(&mut vcx, handle.into(), logs_button_id(LogsPanels::Reuse));
    assert_eq!(
        cx.update(|cx| crate::ui::logs_panels::current(cx)),
        LogsPanels::Reuse
    );
    press_button(&mut vcx, handle.into(), logs_button_id(LogsPanels::PerPod));
    assert_eq!(
        cx.update(|cx| crate::ui::logs_panels::current(cx)),
        LogsPanels::PerPod
    );
}

/// #166: each sidebar button spans the sidebar - so its selected background
/// does - and its title starts at the left, whole: at the default text size
/// the title's natural width fits the room the button gives it, so nothing
/// is ellipsized.
#[gpui_kit::test]
async fn sidebar_buttons_span_the_sidebar_with_whole_left_aligned_titles(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    let main = app(cx);
    let (handle, _) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    for section in [
        Section::KeyboardShortcuts,
        Section::Appearance,
        Section::Panels,
    ] {
        // The title's natural width, in the font and size the window draws
        // it in, and the button's bounds, from a fresh frame.
        let (natural, button) = vcx
            .update_window(handle.into(), |_, window, cx| {
                window.render_frame(cx);
                let theme = cx.theme();
                let title: gpui_kit::SharedString = section.title().into();
                let natural = window
                    .text_system()
                    .shape_line(
                        title.clone(),
                        theme.font_size,
                        &[gpui_kit::TextRun {
                            len: title.len(),
                            font: gpui_kit::font(theme.font_family.clone()),
                            color: theme.foreground,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width;
                let button = window
                    .try_find(ElementId::Name(section.button_id().into()))
                    .unwrap_or_else(|| panic!("{} is drawn", section.button_id()))
                    .bounds();
                (natural, button)
            })
            .unwrap();
        let sidebar = vcx
            .debug_bounds(super::super::SIDEBAR)
            .expect("the sidebar is drawn");
        let label = vcx
            .debug_bounds(section.label_selector())
            .expect("the title is drawn");
        // The same padding both sides, plus the sidebar's 1px right border.
        let inset = button.left() - sidebar.left();
        assert!(
            (sidebar.right() - button.right() - inset - gpui_kit::px(1.)).abs() < gpui_kit::px(0.5),
            "{section:?} spans the sidebar, inset alike both sides: \
             sidebar {sidebar:?}, button {button:?}"
        );
        assert!(
            label.left() - button.left() < gpui_kit::px(20.),
            "{section:?}'s title starts at the left: button {button:?}, title {label:?}"
        );
        assert!(
            natural <= label.size.width,
            "{section:?}'s title fits whole: needs {natural:?}, has {:?}",
            label.size.width
        );
    }
}
