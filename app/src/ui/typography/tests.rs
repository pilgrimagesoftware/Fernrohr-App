//! `visual-refresh-typography-spacing` 2.1 and 2.3: the bundled families register,
//! the three roles resolve to them, and every face is static with one per weight.

use super::recorder::with_recorded_text;
use super::{BUNDLED_FONTS, DATA_FAMILY, FRAME_FAMILY, TypeRole as _};
use crate::config::ui::Theme as ThemePreference;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AvailableSpace, ParentElement as _, Styled as _, div, point, px, size};

/// `font`'s `tag` table, if it has one.
fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let u16_at = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
    (0..u16_at(4))
        .map(|table| 12 + table * 16)
        .find(|&record| &font[record..record + 4] == tag)
        .map(|record| &font[u32_at(record + 8)..][..u32_at(record + 12)])
}

/// The family a TrueType/OpenType file declares, the way the platform text
/// systems resolve it: the name table's typographic family (name ID 16) when
/// the file has one, else its legacy family (name ID 1). A static face of a
/// big family needs the first - Manrope SemiBold's legacy family is
/// "Manrope SemiBold". Decodes UTF-16BE (platforms 0 and 3) or single-byte
/// (platform 1). Enough of the format to read a family name, nothing more.
fn declared_families(font: &[u8]) -> Vec<String> {
    let name = table(font, b"name").expect("a font file has a name table");
    let u16_at = |at: usize| u16::from_be_bytes([name[at], name[at + 1]]) as usize;
    let strings = u16_at(4);
    let records: Vec<usize> = (0..u16_at(2)).map(|ix| 6 + ix * 12).collect();
    let name_id = if records.iter().any(|&record| u16_at(record + 6) == 16) {
        16
    } else {
        1
    };
    let mut families: Vec<String> = records
        .into_iter()
        .filter(|&record| u16_at(record + 6) == name_id)
        .map(|record| {
            let bytes = &name[strings + u16_at(record + 10)..][..u16_at(record + 8)];
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

/// A face's weight, as the platform's font matcher sees it: the OS/2 table's
/// `usWeightClass` (400 regular, 700 bold).
fn weight_class(font: &[u8]) -> u16 {
    let os2 = table(font, b"OS/2").expect("a font file has an OS/2 table");
    u16::from_be_bytes([os2[4], os2[5]])
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

/// The reader above, on the bundled files, so a wrong offset shows up as a
/// wrong name here rather than as a puzzling role failure. Every file but
/// Adamina's is a Manrope face.
#[test]
fn the_bundled_files_declare_adamina_and_manrope() {
    let (adamina, manrope) = BUNDLED_FONTS.split_first().unwrap();
    assert_eq!(declared_families(adamina), ["Adamina"]);
    for face in manrope {
        assert_eq!(declared_families(face), ["Manrope"]);
    }
}

/// 2.3: GPUI has no font-variation support, so a variable file draws every
/// weight at its default instance - the thin text 1.1 traced. No bundled
/// file may carry an `fvar` table.
#[test]
fn no_bundled_font_is_variable() {
    for (ix, font) in BUNDLED_FONTS.iter().enumerate() {
        assert!(
            table(font, b"fvar").is_none(),
            "BUNDLED_FONTS[{ix}] ({:?}) is a variable font",
            declared_families(font)
        );
    }
}

/// 2.3: every weight data text is set in has a Manrope face of its own, so
/// the platform's matcher resolves semibold and regular to different faces
/// rather than drawing both from one. Asserted on the bundled files' weight
/// classes: the test text system resolves any font to the same face.
#[test]
fn every_data_weight_has_its_own_manrope_face() {
    use gpui_kit::FontWeight;

    let manrope: Vec<u16> = BUNDLED_FONTS
        .iter()
        .filter(|font| declared_families(font) == [DATA_FAMILY])
        .map(|font| weight_class(font))
        .collect();
    let face_for = |weight: FontWeight| {
        let class = weight.0 as u16;
        let faces: Vec<usize> = (0..manrope.len())
            .filter(|&ix| manrope[ix] == class)
            .collect();
        assert_eq!(
            faces.len(),
            1,
            "one Manrope face at weight {class}, in {manrope:?}"
        );
        faces[0]
    };
    for weight in [
        FontWeight::NORMAL,
        FontWeight::MEDIUM,
        FontWeight::SEMIBOLD,
        FontWeight::BOLD,
    ] {
        face_for(weight);
    }
    assert_ne!(
        face_for(FontWeight::SEMIBOLD),
        face_for(FontWeight::NORMAL),
        "semibold and regular resolve to different faces"
    );
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
