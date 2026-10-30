//! A hint's look (#251): muted, smaller than a control's text, and in the data
//! font rather than the frame font around it.

use super::hint;
use crate::ui::typography::DATA_FAMILY;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{Styled as _, TestAppContext, rems};

#[gpui_kit::test]
fn a_hint_is_muted_small_and_in_the_data_font(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut hint = hint("Shift-L opens a pod's logs the other way, once.", cx);
        let text = &hint.style().text;
        assert_eq!(
            text.font_family.as_ref().map(|family| family.as_ref()),
            Some(DATA_FAMILY),
            "the data font"
        );
        assert_eq!(text.color, Some(cx.theme().muted_foreground), "muted");
        assert_eq!(text.font_size, Some(rems(0.75).into()), "text_xs");
    });
}
