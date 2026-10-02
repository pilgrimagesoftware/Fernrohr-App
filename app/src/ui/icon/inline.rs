//! A [`KindIcon`] that sizes itself to the text it sits in, for places that
//! build their elements with only `&App` - resource links, whose text size
//! is whatever their caller's is.
//!
//! [`kind_icon`] needs the window and `&mut App` to rasterize, and a role to
//! size by. This defers both to layout: GPUI renders a [`RenderOnce`] inside
//! its parent's text style, so the inherited font size picks the role.

use super::{IconSize, KindIcon, kind_icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// `icon`, sized to the surrounding text. Decorative, like [`kind_icon`].
#[derive(IntoElement)]
pub struct InlineKindIcon {
    icon: KindIcon,
    selector: Option<SharedString>,
}

impl InlineKindIcon {
    pub fn new(icon: KindIcon) -> Self {
        Self {
            icon,
            selector: None,
        }
    }

    /// Names the icon for tests' `debug_bounds`; a no-op in release builds.
    pub fn selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.selector = Some(selector.into());
        self
    }
}

impl RenderOnce for InlineKindIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = window.text_style().font_size.to_pixels(window.rem_size());
        let size = role_for(text, cx);
        div()
            .flex_none()
            .when_some(self.selector, |this, selector| {
                this.debug_selector(move || selector.to_string())
            })
            .child(kind_icon(self.icon, size, window, cx))
    }
}

/// The role whose size, at the current text size, is nearest `text`. The
/// app's text is always one of the three roles, so this is exact in practice;
/// a larger heading gets the body role's icon.
pub(super) fn role_for(text: Pixels, cx: &App) -> IconSize {
    let distance = |role: IconSize| f32::from((role.logical(cx) - text).abs());
    [IconSize::Text, IconSize::Small, IconSize::XSmall]
        .into_iter()
        .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
        .expect("three roles")
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
