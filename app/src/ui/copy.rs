//! Copying from the detail panels (`resource-detail-ui-improvements` 3): the
//! Copy Resource Name command, and a copy control beside a copyable value - a
//! container image, a ConfigMap key or value, a revealed Secret value.
//!
//! The control sits beside its value, faint until the row is hovered, so a row
//! of values doesn't read as a row of buttons. It is a tab stop, so the
//! keyboard reaches it and Space or Enter copies, as a click does.
//!
//! Every copy shows it worked (#187): for [`COPIED_FEEDBACK`] the control
//! shows a check mark and says "Copied" ([`CopiedFeedback`]), fully visible
//! even outside a hovered row, so a keyboard copy is seen too. A metadata chip
//! (`ui::detail::metadata`) shows the same.

use crate::consts::COPIED_FEEDBACK;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

actions!(copy, [CopyResourceName]);

/// Copy Resource Name's default key in each detail panel.
pub const COPY_NAME_KEY: &str = "c";

/// How visible a copy control is while its row isn't hovered.
const RESTING_OPACITY: f32 = 0.35;

/// Puts `text` on the clipboard.
pub fn copy_text(text: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
}

/// A copy control's "it copied" state, kept per control across renders: shown
/// from a copy for [`COPIED_FEEDBACK`], then gone.
pub struct CopiedFeedback {
    shown: bool,
    /// Which copy the pending hide belongs to, so a copy made while one shows
    /// gets its full time rather than the earlier copy's remainder.
    generation: u64,
}

impl CopiedFeedback {
    /// The feedback of the copy control `id`. Its owning view re-renders when
    /// it changes.
    pub fn of(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<Self> {
        window.use_keyed_state(
            ElementId::Name(format!("copied-feedback {id}").into()),
            cx,
            |_, _| Self {
                shown: false,
                generation: 0,
            },
        )
    }

    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Shows the feedback now, and hides it after [`COPIED_FEEDBACK`].
    pub fn show(&mut self, cx: &mut Context<Self>) {
        self.shown = true;
        self.generation += 1;
        let generation = self.generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FEEDBACK).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.shown = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

/// Copies `text` and shows `feedback`.
pub fn copy_with_feedback(text: &str, feedback: &Entity<CopiedFeedback>, cx: &mut App) {
    copy_text(text, cx);
    feedback.update(cx, CopiedFeedback::show);
}

/// What a copy control's tooltip says, by whether it just copied.
pub fn tooltip_text(copied: bool) -> &'static str {
    if copied { "Copied" } else { "Copy" }
}

/// The debug selector of the check mark the copy control `id` shows while it
/// shows it copied - what tests look for.
pub fn copied_selector(id: &ElementId) -> String {
    format!("copied {id}")
}

/// A copy control for `value`, `id` its element id. It brightens while the
/// element with `group` - the value's row - is hovered, and while it shows it
/// copied.
pub fn copy_button(
    id: impl Into<ElementId>,
    value: String,
    group: impl Into<SharedString>,
) -> AnyElement {
    CopyButton {
        id: id.into(),
        value,
        group: group.into(),
    }
    .into_any_element()
}

/// [`copy_button`], an element of its own for the window its feedback state
/// lives in.
#[derive(IntoElement)]
struct CopyButton {
    id: ElementId,
    value: String,
    group: SharedString,
}

impl RenderOnce for CopyButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { id, value, group } = self;
        let feedback = CopiedFeedback::of(&id, window, cx);
        let copied = feedback.read(cx).shown();
        let selector = copied_selector(&id);
        div()
            .flex_none()
            .when(copied, |this| this.debug_selector(move || selector))
            .when(!copied, |this| this.opacity(RESTING_OPACITY))
            .group_hover(group, |style| style.opacity(1.))
            .child(
                Button::new(id)
                    .icon(if copied {
                        IconName::Check
                    } else {
                        IconName::Copy
                    })
                    .xsmall()
                    .ghost()
                    .tooltip(tooltip_text(copied))
                    .on_click(move |_event, _window, cx| copy_with_feedback(&value, &feedback, cx)),
            )
    }
}

/// `content` with a copy control for `value` after it, the pair one hover
/// group named `group`.
pub fn copyable(
    content: impl IntoElement,
    id: impl Into<ElementId>,
    value: String,
    group: impl Into<SharedString>,
) -> AnyElement {
    let group = group.into();
    div()
        .group(group.clone())
        .flex()
        .items_center()
        .gap_1()
        .min_w_0()
        .w_full()
        .child(content)
        .child(copy_button(id, value, group))
        .into_any_element()
}
