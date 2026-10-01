//! Drawing the Configuration tab: one card per referenced ConfigMap or
//! Secret, with its link, its uses and its contents, a reveal button per
//! Secret key, and an expand button per large ConfigMap value.

use super::state::CardContents;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::ui::detail::{self, Collapsible};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::rc::Rc;

/// The id of the reveal button for key `index` of card `card` - what a click
/// targets, and what a test finds the button by.
pub(in crate::k8s::resource::pod_detail) fn reveal_button_id(
    card: usize,
    index: usize,
) -> ElementId {
    ElementId::NamedInteger(format!("reveal-{card}").into(), index as u64)
}

/// The id of the expand button for ConfigMap key `index` of card `card`.
pub(in crate::k8s::resource::pod_detail) fn expand_button_id(
    card: usize,
    index: usize,
) -> ElementId {
    ElementId::NamedInteger(format!("expand-{card}").into(), index as u64)
}

/// The id of ConfigMap key `index` of card `card`'s label.
pub(in crate::k8s::resource::pod_detail) fn key_label_id(card: usize, index: usize) -> ElementId {
    ElementId::NamedInteger(format!("key-{card}").into(), index as u64)
}

/// The id of the drawn value of key `index` of card `card` - a ConfigMap value,
/// or a Secret value while revealed.
pub(in crate::k8s::resource::pod_detail) fn value_id(card: usize, index: usize) -> ElementId {
    ElementId::NamedInteger(format!("value-{card}").into(), index as u64)
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
                    .children(data.iter().enumerate().map(|(index, (key, value))| {
                        self.collapsible(card, index, target, key, cx).render(
                            key,
                            value.clone(),
                            cx,
                        )
                    }))
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
        let this = cx.weak_entity();
        let (target, owned_key) = (secret.clone(), key.to_string());
        detail::secret_key_row(
            key,
            size,
            self.configuration.reveal_of(secret, key),
            reveal_button_id(card, index),
            value_id(card, index),
            move |_window, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.toggle_reveal(target.clone(), owned_key.clone(), cx)
                });
            },
            cx,
        )
    }

    /// ConfigMap value `index` of card `card`'s element and expand control,
    /// wired to this panel's expansion state.
    fn collapsible(
        &self,
        card: usize,
        index: usize,
        target: &ObjectRef,
        key: &str,
        cx: &Context<Self>,
    ) -> Collapsible {
        let this = cx.weak_entity();
        let (target_owned, owned_key) = (target.clone(), key.to_string());
        Collapsible {
            key_id: key_label_id(card, index),
            value_id: value_id(card, index),
            toggle_id: expand_button_id(card, index),
            expanded: self.configuration.is_expanded(target, key),
            on_toggle: Rc::new(move |_window, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.toggle_expanded(target_owned.clone(), owned_key.clone(), cx)
                });
            }),
        }
    }
}
