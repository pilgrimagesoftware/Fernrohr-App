use super::{current, init, set};
use crate::config::{self, ui::TextSize, ui::Theme as ThemePreference, ui::UiConfig};
use crate::consts::{DEFAULT_FONT_SIZE, DEFAULT_MONO_FONT_SIZE};
use crate::ui::space::{Spacing, TextScale, spacing};
use gpui_kit::component::{Root, Theme};
use gpui_kit::{
    App, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};

fn temp_path() -> std::path::PathBuf {
    // A `ui.toml` in a directory of its own, as the preference file sits.
    crate::util::test_paths::temp_path("text-size")
        .with_extension("")
        .join("ui.toml")
}

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(ThemePreference::Light, cx);
    });
}

fn font_sizes(cx: &App) -> (Pixels, Pixels) {
    let theme = cx.global::<Theme>();
    (theme.font_size, theme.mono_font_size)
}

#[gpui_kit::test]
async fn setting_a_size_scales_every_role_and_the_spacing(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
        assert_eq!(
            font_sizes(cx),
            (px(DEFAULT_FONT_SIZE), px(DEFAULT_MONO_FONT_SIZE))
        );
        set(TextSize::MAX, cx);
        assert_eq!(current(cx), TextSize::MAX);
        assert_eq!(font_sizes(cx), (px(24.), px(19.5)));
        assert_eq!(TextScale::current(cx).factor(), 1.5);
        assert_eq!(
            spacing(cx),
            Spacing::at(TextScale::new(1.5).expect("positive"))
        );

        set(TextSize::DEFAULT, cx);
        assert_eq!(
            font_sizes(cx),
            (px(DEFAULT_FONT_SIZE), px(DEFAULT_MONO_FONT_SIZE))
        );
        assert_eq!(TextScale::current(cx), TextScale::DEFAULT);
    });
}

#[gpui_kit::test]
async fn a_theme_mode_change_keeps_the_text_size(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
        set(TextSize::from(130), cx);
        crate::ui::theme::init(ThemePreference::Dark, cx);
        assert_eq!(font_sizes(cx), (px(16. * 1.3), px(13. * 1.3)));
    });
}

#[gpui_kit::test]
async fn a_change_is_saved_without_touching_the_rest_of_the_file(cx: &mut TestAppContext) {
    setup(cx);
    let path = temp_path();
    let stored = UiConfig {
        theme: ThemePreference::Dark,
        text_size: TextSize::DEFAULT,
        resource_side: crate::config::ui::ResourceSide::Right,
        ..UiConfig::default()
    };
    config::save(&path, &stored).expect("temp file written");
    cx.update(|cx| {
        init(stored.text_size, path.clone(), cx);
        set(TextSize::from(120), cx);
    });

    let reloaded: UiConfig = config::load(&path);
    assert_eq!(reloaded.text_size, TextSize::from(120));
    assert_eq!(reloaded.theme, ThemePreference::Dark);
    assert_eq!(reloaded.resource_side, stored.resource_side);

    // A relaunch starts at the saved size.
    let mut relaunch = TestAppContext::single();
    setup(&mut relaunch);
    relaunch.update(|cx| {
        init(reloaded.text_size, path.clone(), cx);
        assert_eq!(current(cx), TextSize::from(120));
        assert_eq!(font_sizes(cx).0, px(16. * 1.2));
    });
}

#[gpui_kit::test]
async fn stepping_past_a_bound_changes_and_saves_nothing(cx: &mut TestAppContext) {
    setup(cx);
    let path = temp_path();
    cx.update(|cx| {
        init(TextSize::MAX, path.clone(), cx);
        set(TextSize::MAX.increase(), cx);
        assert_eq!(current(cx), TextSize::MAX);
    });
    assert!(!path.exists(), "nothing should have been written");
}

struct Probe;

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // `flex` so the probe is as wide as its text, not the window.
        div().flex().child(
            div()
                .debug_selector(|| "text-size-probe".into())
                .text_sm()
                .child("nginx-7d4b9c-x2kq"),
        )
    }
}

#[gpui_kit::test]
async fn an_open_window_redraws_larger(cx: &mut TestAppContext) {
    setup(cx);
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Probe);
        Root::new(view, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let before = vcx.debug_bounds("text-size-probe").expect("probe drawn");

    // No explicit refresh of the window: `set` has to redraw it.
    vcx.update(|_, cx| set(TextSize::MAX, cx));
    vcx.run_until_parked();
    let after = vcx.debug_bounds("text-size-probe").expect("probe drawn");

    assert!(
        after.size.height > before.size.height && after.size.width > before.size.width,
        "{before:?} -> {after:?}"
    );
}
