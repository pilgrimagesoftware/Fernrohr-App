//! #187: a chip shows it copied - a check mark in it, for a moment - and only
//! that chip. The keyboard routes (Tab, then Space or Enter) are tested in a
//! real panel, `pod_detail::tests::metadata_chips`, where Tab moves focus.

use super::{metadata_chip_id, metadata_chip_selector, metadata_chips};
use crate::consts::COPIED_FEEDBACK;
use crate::ui::copy::copied_selector;
use gpui_kit::{
    Context, IntoElement, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};

/// Two chips, side by side.
struct Chips;

impl Render for Chips {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(800.)).child(metadata_chips(
            "Labels",
            &[
                ("app".to_string(), "web".to_string()),
                ("tier".to_string(), "front".to_string()),
            ],
        ))
    }
}

fn chips(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
    });
    let (_, vcx) = cx.add_window_view(|_, _| Chips);
    vcx
}

/// Whether chip `index` shows it copied.
fn shows_copied(vcx: &mut VisualTestContext, index: usize) -> bool {
    vcx.run_until_parked();
    let selector = copied_selector(&metadata_chip_id("Labels", index));
    vcx.debug_bounds(selector.leak()).is_some()
}

fn clipboard(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui_kit::test]
async fn a_clicked_chip_shows_it_copied_then_stops(cx: &mut TestAppContext) {
    let vcx = chips(cx);
    assert!(!shows_copied(vcx, 0), "nothing copied yet");

    let chip = vcx
        .debug_bounds(metadata_chip_selector("Labels", 0).leak())
        .expect("the chip is drawn");
    vcx.simulate_click(chip.center(), Modifiers::none());

    assert_eq!(clipboard(vcx).as_deref(), Some("app=web"));
    assert!(shows_copied(vcx, 0), "the clicked chip shows it copied");
    assert!(!shows_copied(vcx, 1), "only that chip");

    vcx.executor().advance_clock(COPIED_FEEDBACK);
    assert!(!shows_copied(vcx, 0), "and stops after a moment");
}

/// A second copy while the first still shows gets its full time, not the
/// first one's remainder.
#[gpui_kit::test]
async fn copying_again_restarts_the_feedback(cx: &mut TestAppContext) {
    let vcx = chips(cx);
    let chip = vcx
        .debug_bounds(metadata_chip_selector("Labels", 0).leak())
        .expect("the chip is drawn");
    vcx.simulate_click(chip.center(), Modifiers::none());
    vcx.run_until_parked();
    vcx.executor().advance_clock(COPIED_FEEDBACK / 2);
    vcx.simulate_click(chip.center(), Modifiers::none());
    vcx.run_until_parked();
    vcx.executor().advance_clock(COPIED_FEEDBACK * 3 / 4);

    assert!(shows_copied(vcx, 0), "the second copy's time isn't up");
    vcx.executor().advance_clock(COPIED_FEEDBACK);
    assert!(!shows_copied(vcx, 0));
}
