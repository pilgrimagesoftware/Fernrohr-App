//! Drawing the Configuration tab: one card per referenced ConfigMap or Secret
//! - its link, its uses, its contents - with a reveal button per Secret key.

use super::state::{CardContents, Reveal};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::secret_value::RevealError;
use crate::ui::detail;
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// The id of the reveal button for key `index` of card `card` - what a click
/// targets, and what a test finds the button by.
pub(in crate::k8s::resource::pod_detail) fn reveal_button_id(
    card: usize,
    index: usize,
) -> ElementId {
    ElementId::NamedInteger(format!("reveal-{card}").into(), index as u64)
}

impl PodDetailPanel {
    pub(in crate::k8s::resource::pod_detail) fn render_configuration(
        &self,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let entries = self.configuration_entries();
        if entries.is_empty() {
            return div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("This pod references no ConfigMaps or Secrets.")
                .into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(entries.iter().enumerate().map(|(card, entry)| {
                let header = crate::ui::link::references(
                    format!("config-{card}"),
                    std::slice::from_ref(&entry.target),
                    ObjectRef::qualified_name,
                    &self.scope.context_name,
                    self.kinds(cx),
                    cx,
                );
                let contents = self
                    .configuration
                    .cards
                    .get(&entry.target)
                    .map(|contents| self.render_contents(card, &entry.target, contents, cx))
                    .unwrap_or_else(|| div().into_any_element());
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(div().font_weight(FontWeight::MEDIUM).child(header))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .children(entry.uses.iter().map(|used| div().child(used.clone()))),
                    )
                    .child(div().pt_1().child(contents))
            }))
            .into_any_element()
    }

    fn render_contents(
        &self,
        card: usize,
        target: &ObjectRef,
        contents: &CardContents,
        cx: &Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let note = |text: String| {
            div()
                .text_sm()
                .text_color(muted)
                .child(text)
                .into_any_element()
        };
        match contents {
            CardContents::Loading => note("Loading…".into()),
            CardContents::NotFound => note(format!(
                "This {} doesn't exist.",
                target.kind.to_lowercase()
            )),
            CardContents::Failed(message) => note(format!("Could not read it: {message}")),
            CardContents::ConfigMap { data, binary } => {
                if data.is_empty() && binary.is_empty() {
                    return note("No data.".into());
                }
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(detail::key_values(data, cx))
                    .children(binary.iter().map(|(key, size)| {
                        div()
                            .text_sm()
                            .child(format!("{key}: binary, {size} bytes"))
                    }))
                    .into_any_element()
            }
            CardContents::Secret { type_, keys } => div()
                .flex()
                .flex_col()
                .gap_1()
                .children(type_.clone().map(|type_| {
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("Type: {type_}"))
                }))
                .children(keys.iter().enumerate().map(|(index, (key, size))| {
                    self.render_secret_key(card, index, target, key, *size, cx)
                }))
                .into_any_element(),
        }
    }

    fn render_secret_key(
        &self,
        card: usize,
        index: usize,
        secret: &ObjectRef,
        key: &str,
        size: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let reveal = self.configuration.reveal_of(secret, key);
        let shown = reveal.is_some();
        let this = cx.weak_entity();
        let (target, owned_key) = (secret.clone(), key.to_string());
        let button = Button::new(reveal_button_id(card, index))
            .icon(if shown {
                IconName::EyeOff
            } else {
                IconName::Eye
            })
            .label(if shown { "Hide" } else { "Show" })
            .xsmall()
            .ghost()
            .on_click(move |_event, _window, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.toggle_reveal(target.clone(), owned_key.clone(), cx)
                });
            });
        let value = reveal.map(|reveal| {
            let text = match reveal {
                Reveal::Pending => "Revealing…".to_string(),
                // The one place a value is read out of a `SecretValue`.
                Reveal::Shown(value) if value.is_empty() => "(empty)".to_string(),
                Reveal::Shown(value) => match value.expose() {
                    Some(text) => text.to_string(),
                    None => format!("binary, {} bytes", value.len()),
                },
                Reveal::Failed(RevealError::Missing) => "It no longer exists.".to_string(),
                Reveal::Failed(RevealError::Failed(message)) => {
                    format!("Could not read it: {message}")
                }
            };
            div()
                .id(ElementId::NamedInteger(
                    format!("revealed-{card}").into(),
                    index as u64,
                ))
                .font_family(theme.mono_font_family.clone())
                .text_sm()
                .child(text)
                .test_support()
        });
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .child(
                        div()
                            .font_family(theme.mono_font_family.clone())
                            .child(key.to_string()),
                    )
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(format!("{size} bytes")),
                    )
                    .child(button),
            )
            .children(value)
            .into_any_element()
    }
}
