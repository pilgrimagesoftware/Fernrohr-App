//! One structured-view row: a field's label and its value, drawn according to
//! the value's shape.

use super::model::{PodField, PodFieldValue, reference_text};
use super::panel::PodDetailPanel;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::detail;
use crate::ui::typography::TypeRole as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl PodDetailPanel {
    pub(super) fn render_field(&self, field: &PodField, cx: &Context<Self>) -> AnyElement {
        let value =
            match &field.value {
                PodFieldValue::Text(text) => div().child(text.clone()).into_any_element(),
                PodFieldValue::Status { text, tone } => div()
                    .debug_selector(|| "pod-status-value".into())
                    .text_color(detail::tone_color(*tone, cx))
                    .child(text.clone())
                    .into_any_element(),
                PodFieldValue::References { targets, qualified } => {
                    self.render_references(field.label, targets, *qualified, cx)
                }
                PodFieldValue::Metadata(pairs) => detail::metadata_chips(field.label, pairs),
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
                                .frame_font(cx)
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
                        .child(Collapsible::new().open(open).content(
                            div().flex().flex_col().children(
                                rows.iter().map(|row| div().text_sm().child(row.clone())),
                            ),
                        ))
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
                // Full width, not a labelled row - see `render_containers`.
                // The main run is the one the tab is named for, so only the
                // init containers get a heading.
                PodFieldValue::Containers(containers) => {
                    let heading = (field.label != "Containers").then_some(field.label);
                    return self.render_containers(heading, containers, cx);
                }
                PodFieldValue::ManagedFields(entries) => {
                    return self.render_managed_fields(entries, cx);
                }
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
