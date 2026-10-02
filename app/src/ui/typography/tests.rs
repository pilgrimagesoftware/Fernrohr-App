//! `visual-refresh-typography-spacing` 2.1: the bundled families register and
//! the three roles resolve to them.

use super::recorder::with_recorded_text;
use super::{BUNDLED_FONTS, DATA_FAMILY, FRAME_FAMILY, TypeRole as _};
use crate::config::ui::Theme as ThemePreference;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AvailableSpace, ParentElement as _, Styled as _, div, point, px, size};

/// The family a TrueType/OpenType file declares, the way the platform text
/// systems resolve it: the name table's typographic family (name ID 16) when
/// the file has one, else its legacy family (name ID 1). A variable font
/// needs the first - Manrope's legacy family is "Manrope ExtraLight", its
/// default instance's name. Decodes UTF-16BE (platforms 0 and 3) or
/// single-byte (platform 1). Enough of the format to read a family name,
/// nothing more.
fn declared_families(font: &[u8]) -> Vec<String> {
    let u16_at = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
    let name_table = (0..u16_at(4))
        .map(|table| 12 + table * 16)
        .find(|&record| &font[record..record + 4] == b"name")
        .map(|record| u32_at(record + 8))
        .expect("a font file has a name table");
    let strings = name_table + u16_at(name_table + 4);
    let records: Vec<usize> = (0..u16_at(name_table + 2))
        .map(|ix| name_table + 6 + ix * 12)
        .collect();
    let name_id = if records.iter().any(|&record| u16_at(record + 6) == 16) {
        16
    } else {
        1
    };
    let mut families: Vec<String> = records
        .into_iter()
        .filter(|&record| u16_at(record + 6) == name_id)
        .map(|record| {
            let bytes = &font[strings + u16_at(record + 10)..][..u16_at(record + 8)];
            match u16_at(record) {
                1 => bytes.iter().map(|&b| char::from(b)).collect(),
                _ => String::from_utf16_lossy(
                    &bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| u16::from_be_bytes(*pair))
                        .collect::<Vec<_>>(),
                ),
            }
        })
        .collect();
    families.sort();
    families.dedup();
    families
}

/// `theme::init` registers exactly the bundled files, and those files are
/// where the frame and data roles' families come from: each role's family is
/// one a registered file declares. The code role is a system font, probed at
/// startup (`ui::theme`), so it is only checked to be set.
#[test]
fn init_registers_the_bundled_files_the_roles_resolve_to() {
    with_recorded_text(|cx, recorded| {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::ui::theme::init(ThemePreference::Light, cx);
        });
        let added = recorded.added_fonts();
        assert_eq!(
            added,
            BUNDLED_FONTS
                .iter()
                .map(|font| font.to_vec())
                .collect::<Vec<_>>(),
            "init registers every bundled font, and nothing else"
        );
        let declared: Vec<String> = added
            .iter()
            .flat_map(|font| declared_families(font))
            .collect();

        let (frame, code) = cx.update(|cx| {
            let theme = cx.theme();
            (
                theme.font_family.to_string(),
                theme.mono_font_family.clone(),
            )
        });
        assert_eq!(frame, FRAME_FAMILY);
        for (role, family) in [("frame", frame.as_str()), ("data", DATA_FAMILY)] {
            assert!(
                declared.iter().any(|name| name == family),
                "the {role} role's {family:?} isn't a family the bundled files declare: {declared:?}"
            );
        }
        assert!(!code.is_empty(), "the code role names a family");
    });
}

/// The reader above, on the two bundled files, so a wrong offset shows up as a
/// wrong name here rather than as a puzzling role failure.
#[test]
fn the_bundled_files_declare_adamina_and_manrope() {
    assert_eq!(declared_families(BUNDLED_FONTS[0]), ["Adamina"]);
    assert_eq!(declared_families(BUNDLED_FONTS[1]), ["Manrope"]);
}

/// Each role, set on an element, is the family its text is drawn in: frame
/// by default (inherited from the theme), data and code where asked for.
#[test]
fn each_role_draws_its_text_in_its_family() {
    with_recorded_text(|cx, recorded| {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::ui::theme::init(ThemePreference::Light, cx);
        });
        let mono = cx.update(|cx| cx.theme().mono_font_family.clone());
        let window = cx.add_empty_window();
        window.draw(
            point(px(0.), px(0.)),
            size(
                AvailableSpace::Definite(px(400.)),
                AvailableSpace::MinContent,
            ),
            |_, cx| {
                div()
                    .font_family(cx.theme().font_family.clone())
                    .child(div().child("frame text"))
                    .child(div().data_font().child("data text"))
                    .child(div().code_font(cx).child("code text"))
                    .child(
                        div()
                            .data_font()
                            .child(div().frame_font(cx).child("frame inside data")),
                    )
            },
        );
        assert_eq!(recorded.family_of("frame text").as_ref(), FRAME_FAMILY);
        assert_eq!(recorded.family_of("data text").as_ref(), DATA_FAMILY);
        assert_eq!(recorded.family_of("code text"), mono);
        assert_eq!(
            recorded.family_of("frame inside data").as_ref(),
            FRAME_FAMILY
        );
    });
}
