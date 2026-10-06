//! An inline icon takes the role of the text around it, so it matches what
//! [`kind_icon`] draws for that role - at any text size.

use super::super::{IconSize, KindIcon, kind_icon};
use super::InlineKindIcon;
use crate::config::ui::{TextSize, Theme as ThemePreference};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    div,
};

struct Sample;

/// An inline icon in `text`, beside the explicit `role` it should match.
/// `items_start` so the row doesn't stretch either icon to its own height.
fn row(
    name: &'static str,
    text: Div,
    role: IconSize,
    window: &mut Window,
    cx: &mut Context<Sample>,
) -> Div {
    div()
        .flex()
        .items_start()
        .child(text.child(InlineKindIcon::new(KindIcon::Pod).selector(format!("{name}-inline"))))
        .child(
            div()
                .debug_selector(move || format!("{name}-explicit"))
                .child(kind_icon(KindIcon::Pod, role, window, cx)),
        )
}

impl Render for Sample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_start()
            .child(row("text", div(), IconSize::Text, window, cx))
            .child(row("small", div().text_sm(), IconSize::Small, window, cx))
    }
}

fn height(vcx: &mut VisualTestContext, selector: String) -> Pixels {
    // `debug_bounds` wants a `'static` selector; a test's few are leaked.
    let selector: &'static str = selector.leak();
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .size
        .height
}

#[gpui_kit::test]
async fn an_inline_icon_takes_its_texts_role_at_every_text_size(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::ui::theme::init(ThemePreference::Light, cx);
    });
    let window: AnyWindowHandle = cx
        .add_window(|window, cx| {
            let view = cx.new(|_| Sample);
            gpui_kit::component::Root::new(view, window, cx)
        })
        .into();
    let mut vcx = VisualTestContext::from_window(window, cx);
    for size in [TextSize::DEFAULT, TextSize::MAX] {
        vcx.update(|_, cx| crate::ui::text_size::set(size, cx));
        vcx.run_until_parked();
        let mut seen = Vec::new();
        for name in ["text", "small"] {
            let inline = height(&mut vcx, format!("{name}-inline"));
            let explicit = height(&mut vcx, format!("{name}-explicit"));
            assert_eq!(inline, explicit, "{name} at {}%", size.percent());
            seen.push(inline);
        }
        assert!(
            seen[0] > seen[1],
            "two distinct roles at {}%: {seen:?}",
            size.percent()
        );
    }
}
