//! Every capsule's text reads at the WCAG text contrast, 4.5:1, in every bundled
//! theme with the app's overrides applied - the attention capsule on its fill,
//! every other capsule's secondary text on the status bar and on the active
//! capsule's selection fill - so a future theme can't regress it.

use super::{SECONDARY_TEXT_REMS, capsule_palette};
use crate::k8s::cluster::context_health::Severity;
use crate::ui::style::{TEXT_CONTRAST, contrast, over};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode};
use gpui_kit::{App, TestAppContext};

/// Every theme the app ships: gpui-component's light and dark, with the app's
/// own overrides (`ui::style::apply_overrides`) applied as at launch.
const BUNDLED: [ThemeMode; 2] = [ThemeMode::Light, ThemeMode::Dark];

fn in_mode(mode: ThemeMode, cx: &mut App) {
    crate::util::test_ui::init(cx);
    Theme::change(mode, None, cx);
    crate::ui::accent::refresh(cx);
}

#[gpui_kit::test]
fn an_attention_capsule_reads_on_its_fill(cx: &mut TestAppContext) {
    for mode in BUNDLED {
        cx.update(|cx| {
            in_mode(mode, cx);
            let palette = capsule_palette(cx.theme(), Severity::Attention);
            let fill = palette.fill.expect("an attention capsule is filled");
            let (primary, secondary) = (
                contrast(palette.primary, fill),
                contrast(palette.secondary, fill),
            );
            assert!(
                primary >= TEXT_CONTRAST,
                "{mode:?}: name and icon at {primary:.2}:1"
            );
            assert!(
                secondary >= TEXT_CONTRAST,
                "{mode:?}: tunnel and elapsed at {secondary:.2}:1"
            );
        });
    }
}

#[gpui_kit::test]
fn secondary_text_reads_on_the_status_bar(cx: &mut TestAppContext) {
    for mode in BUNDLED {
        cx.update(|cx| {
            in_mode(mode, cx);
            let theme = cx.theme().clone();
            let surfaces = [
                ("the status bar", theme.background),
                ("an active capsule", over(theme.selection, theme.background)),
            ];
            for severity in [
                Severity::Muted,
                Severity::Info,
                Severity::Warning,
                Severity::Danger,
            ] {
                let palette = capsule_palette(&theme, severity);
                assert_eq!(palette.fill, None, "{severity:?} draws on the bar");
                for (surface, background) in surfaces {
                    let ratio = contrast(palette.secondary, background);
                    assert!(
                        ratio >= TEXT_CONTRAST,
                        "{mode:?} {severity:?} secondary on {surface}: {ratio:.2}:1"
                    );
                }
            }
        });
    }
}

/// The secondary text stays a step below the name's `text_xs` (0.75rem), as
/// `status-capsule-icons` asks, without dropping back to the old 0.625rem -
/// checked when the tests compile.
const _: () = {
    const NAME_REMS: f32 = 0.75;
    assert!(SECONDARY_TEXT_REMS < NAME_REMS);
    assert!(SECONDARY_TEXT_REMS >= 0.6875);
};
