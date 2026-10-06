use super::{DISMISS_TOOLTIP, Refusal, render};
use crate::k8s::resource::resource_actions::ActionFailure;
use crate::ui::typography::recorder::with_recorded_text;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, IntoElement, Modifiers, ParentElement as _, Render, Styled as _,
    VisualTestContext, Window, div,
};

const BANNER: &str = "test-refusal";
const DISMISS: &str = "test-refusal-dismiss";

struct Sample;

impl Render for Sample {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let refusal = Refusal {
            action: "Delete Secret db-password".into(),
            failure: ActionFailure {
                message: "Forbidden".into(),
                detail: String::new(),
            },
        };
        div()
            .size_full()
            .child(render(refusal, BANNER, DISMISS, |_| {}, cx))
    }
}

/// Dismiss shows only an icon, so hovering it names the action in a tooltip.
#[test]
fn dismiss_is_an_icon_button_with_a_tooltip() {
    with_recorded_text(|cx, recorded| {
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        });
        // Buttons' tooltips are managed by the window's `Root`.
        let window = cx.add_window(|window, cx| {
            let sample = cx.new(|_| Sample);
            gpui_kit::component::Root::new(sample, window, cx)
        });
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        assert!(
            recorded.families_of(DISMISS_TOOLTIP).is_none(),
            "no text label - only an icon"
        );
        let button = vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(DISMISS).expect("drawn").bounds()
        });
        vcx.simulate_mouse_move(button.center(), None, Modifiers::none());
        vcx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        assert!(
            recorded.families_of(DISMISS_TOOLTIP).is_some(),
            "hovering shows the {DISMISS_TOOLTIP:?} tooltip"
        );
    });
}
