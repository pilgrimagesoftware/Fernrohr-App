//! `collapse-large-metadata-values` 1.1-1.2: which chips shorten and what they
//! show, and that only a shortened chip has a tooltip, carrying the full value.

use super::{MetadataChip, metadata_chip_selector, metadata_chips, metadata_tooltip_selector};
use gpui_kit::{
    Context, IntoElement, Modifiers, Render, TestAppContext, VisualTestContext, Window, div,
};
use gpui_kit::{ParentElement as _, Styled as _, px};
use std::time::Duration;

#[test]
fn a_multi_line_value_shortens_to_its_first_line() {
    let chip = MetadataChip::new("ad.datadoghq.com/checks", "{\n  \"nginx\": {}\n}");
    assert_eq!(chip.text, "ad.datadoghq.com/checks={…");
    assert_eq!(chip.tooltip.as_deref(), Some("{\n  \"nginx\": {}\n}"));
}

#[test]
fn a_long_single_line_value_shortens_to_twenty_characters() {
    let value = "x".repeat(300);
    let chip = MetadataChip::new("sidecar/status", &value);
    assert_eq!(chip.text, format!("sidecar/status={}…", "x".repeat(20)));
    assert_eq!(chip.tooltip, Some(value));
}

#[test]
fn a_value_of_exactly_one_hundred_characters_is_whole() {
    let value = "y".repeat(100);
    let chip = MetadataChip::new("note", &value);
    assert_eq!(chip.text, format!("note={value}"));
    assert_eq!(chip.tooltip, None);
}

#[test]
fn a_multi_byte_value_is_cut_on_a_character_boundary() {
    let value = "é".repeat(150);
    let chip = MetadataChip::new("greeting", &value);
    assert_eq!(chip.text, format!("greeting={}…", "é".repeat(20)));
}

const LONG: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab";

/// A short chip and a shortened one, side by side.
struct Chips;

impl Render for Chips {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(800.)).child(metadata_chips(
            "Annotations",
            &[
                ("app".to_string(), "web".to_string()),
                ("blob".to_string(), LONG.to_string()),
            ],
            cx,
        ))
    }
}

/// Hovers chip `index` and waits out the tooltip delay, returning whether its
/// tooltip is drawn.
fn hover(vcx: &mut VisualTestContext, index: usize) -> bool {
    let chip = vcx
        .debug_bounds(metadata_chip_selector("Annotations", index).leak())
        .expect("the chip is drawn");
    vcx.simulate_mouse_move(chip.center(), None, Modifiers::none());
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    vcx.debug_bounds(metadata_tooltip_selector("Annotations", index).leak())
        .is_some()
}

#[gpui_kit::test]
async fn only_a_shortened_chip_has_a_tooltip(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
    });
    let (_, vcx) = cx.add_window_view(|_, _| Chips);

    assert!(!hover(vcx, 0), "a short chip has no tooltip");
    assert!(
        hover(vcx, 1),
        "a shortened chip shows its full value on hover"
    );
}
