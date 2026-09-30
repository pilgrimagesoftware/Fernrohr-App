//! One structured-view row: a field's label and its value, drawn according to
//! the value's shape.

use super::model::{PodField, PodFieldValue, reference_text};
use super::panel::PodDetailPanel;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::detail;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl PodDetailPanel {
    pub(super) fn render_field(&self, field: &PodField, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let value = match &field.value {
            PodFieldValue::Text(text) => div().child(text.clone()).into_any_element(),
            PodFieldValue::References { targets, qualified } => {
                self.render_references(field.label, targets, *qualified, cx)
            }
            PodFieldValue::Chips(chips) => detail::chips(chips, cx),
            PodFieldValue::Badges(badges) => detail::badges(
                badges
                    .iter()
                    .map(|badge| (badge.condition.as_str(), badge.tone)),
                cx,
            ),
            PodFieldValue::Collapsed(rows) => {
                // Bound before the closure: the id is used twice and the
                // closure is `'static`, so it cannot borrow `field`.
                let label = field.label;
                let open = self.open_sections.contains(label);
                let this = cx.weak_entity();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        Button::new(label)
                            .label(if open { "Hide" } else { "Show" })
                            .xsmall()
                            .ghost()
                            .tab_stop(false)
                            .on_click(move |_event, _window, cx| {
                                let _ = this.update(cx, |this: &mut Self, cx| {
                                    if !this.open_sections.remove(label) {
                                        this.open_sections.insert(label.to_string());
                                    }
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        Collapsible::new().open(open).content(
                            div().flex().flex_col().children(
                                rows.iter().map(|row| div().text_sm().child(row.clone())),
                            ),
                        ),
                    )
                    .into_any_element()
            }
            PodFieldValue::Volumes(volumes) => div()
                .flex()
                .flex_col()
                .children(volumes.iter().map(|volume| {
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x_1()
                        .text_sm()
                        .child(format!("{}: {}", volume.name, volume.source))
                        .when_some(volume.detail.clone(), |this, detail| {
                            this.child(format!(": {detail}"))
                        })
                        .when(!volume.references.is_empty(), |this| {
                            this.child(":").child(self.render_references(
                                format!("volume-{}", volume.name),
                                &volume.references,
                                false,
                                cx,
                            ))
                        })
                }))
                .into_any_element(),
            PodFieldValue::Containers(containers) => div()
                .flex()
                .flex_col()
                .gap_2()
                .children(containers.iter().map(|container| {
                    let ready_color = match container.ready {
                        Some(true) => theme.success,
                        Some(false) => theme.warning,
                        None => theme.muted_foreground,
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().size(px(8.)).rounded_full().bg(ready_color))
                                .child(
                                    div()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(container.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child(container.state.clone()),
                                ),
                        )
                        .child(div().text_sm().min_w_0().child(container.image.clone()))
                        .when(container.restart_count > 0, |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(theme.warning)
                                    .child(format!("{} restarts", container.restart_count)),
                            )
                        })
                        .when(!container.ports.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("Ports: {}", container.ports.join(", "))),
                            )
                        })
                        .when(!container.requests.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("Requests: {}", container.requests.join(", "))),
                            )
                        })
                        .when(!container.limits.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("Limits: {}", container.limits.join(", "))),
                            )
                        })
                        .when(!container.env_sources.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_x_1()
                                    .text_sm()
                                    .child(
                                        div().text_color(theme.muted_foreground).child("Env from:"),
                                    )
                                    .child(self.render_references(
                                        format!("env-{}", container.name),
                                        &container.env_sources,
                                        true,
                                        cx,
                                    )),
                            )
                        })
                }))
                .into_any_element(),
            PodFieldValue::ManagedFields(entries) => div()
                .flex()
                .flex_col()
                .gap_2()
                .children(entries.iter().enumerate().map(|(index, entry)| {
                    // Keyed by index, not manager name: two entries can
                    // share a manager (a status subresource update versus
                    // the main resource), and collapsing them onto one key
                    // would toggle both at once.
                    let key: SharedString = format!("mf-{index}").into();
                    let open = self.open_sections.contains(key.as_ref());
                    let this = cx.weak_entity();
                    let key_for_click = key.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(format!("{}: {}", entry.manager, entry.operation))
                                .child(
                                    Button::new(key)
                                        .label(if open { "Hide" } else { "Show" })
                                        .xsmall()
                                        .ghost()
                                        .tab_stop(false)
                                        .on_click(move |_event, _window, cx| {
                                            let _ = this.update(cx, |this: &mut Self, cx| {
                                                if !this
                                                    .open_sections
                                                    .remove(key_for_click.as_ref())
                                                {
                                                    this.open_sections
                                                        .insert(key_for_click.to_string());
                                                }
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            // Wraps rather than `whitespace_nowrap()` like the
                            // YAML view: the structured view scrolls vertically
                            // only, so an unwrapped deep ownership path would
                            // push the panel wider than it is.
                            Collapsible::new().open(open).content(
                                div()
                                    .font_family(theme.mono_font_family.clone())
                                    .text_sm()
                                    .child(entry.fields_json.clone()),
                            ),
                        )
                        .into_any_element()
                }))
                .into_any_element(),
        };

        detail::row(field.label, value, cx)
    }

    /// A run of references, each a link when it can be followed - see
    /// `ui::link`. `id_prefix` keeps each row's reference ids distinct.
    pub(super) fn render_references(
        &self,
        id_prefix: impl Into<SharedString>,
        targets: &[ObjectRef],
        qualified: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        crate::ui::link::references(
            id_prefix,
            targets,
            |target| reference_text(target, qualified),
            &self.scope.context_name,
            self.kinds(cx),
            cx,
        )
    }
}
