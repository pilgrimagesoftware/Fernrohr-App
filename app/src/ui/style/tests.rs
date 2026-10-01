//! Tests for the visual tokens: the WCAG ratio, surface derivation, and the
//! contrast floor over both modes' themes with the overrides applied.

use super::{
    MARK_CONTRAST, TEXT_CONTRAST, Tone, accent, accent_subtle, contrast, meet, over, status,
    stripe, surface_card, surface_raised,
};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode};
use gpui_kit::{App, Hsla, TestAppContext, hsla, rgb};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.02
}

#[test]
fn contrast_matches_known_wcag_pairs() {
    let white = hsla(0., 0., 1., 1.);
    let black = hsla(0., 0., 0., 1.);
    assert!(close(contrast(white, black), 21.), "white on black is 21:1");
    assert!(
        close(contrast(white, white), 1.),
        "a colour on itself is 1:1"
    );
    // #767676 on white is the classic "just passes AA" grey, 4.54:1.
    assert!(close(contrast(rgb(0x767676).into(), white), 4.54));
    assert!(
        close(contrast(black, white), contrast(white, black)),
        "symmetric"
    );
}

#[test]
fn a_translucent_colour_is_composited_before_comparing() {
    let white = hsla(0., 0., 1., 1.);
    let half_black = hsla(0., 0., 0., 0.5);
    let composited = over(half_black, white);
    assert!((composited.to_rgb().r - 0.5).abs() < 0.01);
    assert!(contrast(half_black, white) < contrast(hsla(0., 0., 0., 1.), white));
}

#[test]
fn meet_moves_a_colour_until_it_passes() {
    let white = hsla(0., 0., 1., 1.);
    let pale_yellow = hsla(0.15, 0.9, 0.85, 1.);
    assert!(
        contrast(pale_yellow, white) < MARK_CONTRAST,
        "the fixture fails as given"
    );
    let met = meet(pale_yellow, white, MARK_CONTRAST);
    assert!(contrast(met, white) >= MARK_CONTRAST);
    assert_eq!(met.h, pale_yellow.h, "hue is kept");
    let passing = hsla(0.6, 0.9, 0.35, 1.);
    assert_eq!(
        meet(passing, white, MARK_CONTRAST),
        passing,
        "a passing colour is left alone"
    );
}

fn in_mode(mode: ThemeMode, cx: &mut App) {
    gpui_kit::init(cx);
    Theme::change(mode, None, cx);
    crate::ui::accent::refresh(cx);
}

/// Raised, card and stripe surfaces step away from the text: lighter than the
/// background in dark mode, darker in light mode, and ordered stripe < raised
/// < card.
#[gpui_kit::test]
fn surfaces_step_away_from_the_text_in_both_modes(cx: &mut TestAppContext) {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|cx| {
            in_mode(mode, cx);
            let background = cx.theme().background.l;
            let (s, r, c) = (stripe(cx).l, surface_raised(cx).l, surface_card(cx).l);
            if mode.is_dark() {
                assert!(
                    background < s && s < r && r < c,
                    "{mode:?}: lighter in steps"
                );
            } else {
                assert!(
                    background > s && s > r && r > c,
                    "{mode:?}: darker in steps"
                );
            }
        });
    }
}

/// The contrast floor, in both modes, with the overrides applied: text on
/// every surface at 4.5:1, the accent and status colours at 3:1.
#[gpui_kit::test]
fn every_mode_meets_the_contrast_floor(cx: &mut TestAppContext) {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|cx| {
            in_mode(mode, cx);
            let theme = cx.theme().clone();
            let surfaces: [(&str, Hsla); 5] = [
                ("background", theme.background),
                ("raised", surface_raised(cx)),
                ("card", surface_card(cx)),
                ("stripe", over(stripe(cx), theme.background)),
                ("selected row", over(accent_subtle(cx), theme.background)),
            ];
            for (name, surface) in surfaces {
                for (text, colour) in [
                    ("text", theme.foreground),
                    ("muted text", theme.muted_foreground),
                ] {
                    let ratio = contrast(colour, surface);
                    assert!(
                        ratio >= TEXT_CONTRAST,
                        "{mode:?}: {text} on {name} is {ratio:.2}:1"
                    );
                }
            }
            let ratio = contrast(accent(cx), theme.background);
            assert!(ratio >= MARK_CONTRAST, "{mode:?}: accent is {ratio:.2}:1");
            for tone in [Tone::Good, Tone::Warning, Tone::Bad] {
                let ratio = contrast(status(tone, cx), theme.background);
                assert!(ratio >= MARK_CONTRAST, "{mode:?}: {tone:?} is {ratio:.2}:1");
            }
            let on_primary = contrast(theme.primary_foreground, theme.primary);
            assert!(
                on_primary >= TEXT_CONTRAST,
                "{mode:?}: button text is {on_primary:.2}:1"
            );
        });
    }
}

/// A system accent too light for the light background is darkened until it
/// meets 3:1, and the theme's own widgets get that darkened accent.
#[gpui_kit::test]
fn a_too_light_system_accent_is_darkened(cx: &mut TestAppContext) {
    cx.update(|cx| {
        in_mode(ThemeMode::Light, cx);
        let pale = hsla(0.15, 0.95, 0.88, 1.);
        crate::ui::accent::set_for_test(Some(pale), cx);
        let background = cx.theme().background;
        assert!(contrast(pale, background) < MARK_CONTRAST);
        assert!(contrast(accent(cx), background) >= MARK_CONTRAST);
        assert_eq!(
            cx.theme().primary,
            accent(cx),
            "the override uses the met accent"
        );
        assert_eq!(cx.theme().link, accent(cx));
        assert_eq!(cx.theme().ring, accent(cx));
        // What gpui-component's widgets read: the resolved tokens.
        let token = gpui_kit::component::ThemeToken::from(accent(cx));
        assert_eq!(cx.theme().tokens.primary, token, "tab strip, switches");
        assert_eq!(cx.theme().tokens.button_primary, token, "primary buttons");
        assert_eq!(
            cx.theme().tokens.table_active,
            gpui_kit::component::ThemeToken::from(super::accent_subtle(cx)),
            "the selected table row"
        );
    });
}

/// The chrome views draw their headings and hint bars on the raised surface,
/// through the token rather than a raw theme fill. A source-level guard: a
/// view that drops the token, or reaches for a raw `muted` fill, fails here.
#[test]
fn chrome_views_use_the_raised_surface_token() {
    let views: [(&str, &str); 7] = [
        ("pods", include_str!("../../k8s/resource/pods/render.rs")),
        (
            "pod detail",
            include_str!("../../k8s/resource/pod_detail/render.rs"),
        ),
        (
            "object detail",
            include_str!("../../k8s/resource/object_detail/render.rs"),
        ),
        ("logs", include_str!("../../util/logs/render.rs")),
        ("placeholder", include_str!("../placeholder.rs")),
        (
            "resource panel",
            include_str!("../panel/resource/render.rs"),
        ),
        ("shortcuts", include_str!("../settings/shortcuts/render.rs")),
    ];
    for (name, source) in views {
        assert!(
            source.contains("surface_raised("),
            "{name} draws its chrome on surface_raised"
        );
        assert!(
            !source.contains(".bg(theme.muted)") && !source.contains(".bg(cx.theme().muted)"),
            "{name} uses no raw muted fill"
        );
    }
}
