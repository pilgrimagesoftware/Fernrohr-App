//! The Containers (and Init Containers) cards: each container's summary,
//! and behind a disclosure chevron its expanded detail - env, volume mounts,
//! probes, command/args, security context.

use super::model::{ContainerDetail, ContainerSummary, EnvValue};
use super::panel::PodDetailPanel;
use crate::ui::detail::{self, BadgeTone};
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The `open_sections` key for container `name`'s card. The `container:`
/// prefix keeps it apart from the field labels (Tolerations) and `mf-<n>`
/// Managed Fields keys sharing the set - a container may well be named
/// `mf-0`, but no container name contains a `:`.
pub(super) fn container_section_key(name: &str) -> String {
    format!("container:{name}")
}

/// The id of container `name`'s disclosure control. Container names are
/// unique across a pod's containers and init containers, so the name alone
/// tells the cards apart.
pub(super) fn container_toggle_id(name: &str) -> ElementId {
    ElementId::Name(format!("container-toggle-{name}").into())
}

/// The id of container `name`'s expanded detail, drawn only while open.
pub(super) fn container_detail_id(name: &str) -> ElementId {
    ElementId::Name(format!("container-detail-{name}").into())
}

impl PodDetailPanel {
    /// One run of container cards across the full width of the tab, with no
    /// label column: the tab already says Containers. `heading` names a
    /// second run (Init Containers) so it stays apart from the first.
    pub(super) fn render_containers(
        &self,
        heading: Option<&'static str>,
        containers: &[ContainerSummary],
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .py_1()
            .px_2()
            .when_some(heading, |this, heading| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(heading),
                )
            })
            .children(
                containers
                    .iter()
                    .map(|container| self.render_container(container, cx)),
            )
            .into_any_element()
    }

    fn render_container(&self, container: &ContainerSummary, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let ready_color = detail::tone_color(
            match container.ready {
                Some(true) => BadgeTone::Good,
                Some(false) => BadgeTone::Warning,
                None => BadgeTone::Unknown,
            },
            cx,
        );
        let key = container_section_key(&container.name);
        let open = self.open_sections.contains(&key);
        let this = cx.weak_entity();
        // A disclosure chevron leading the header, as the resource panel's
        // groups have. A tab stop, unlike the Managed Fields toggles: Tab
        // then Space is its keyboard route, as for the Configuration tab's
        // expand buttons.
        let toggle = Button::new(container_toggle_id(&container.name))
            .icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .tooltip(if open { "Hide details" } else { "Show details" })
            .xsmall()
            .ghost()
            .on_click(move |_event, _window, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    if !this.open_sections.remove(&key) {
                        this.open_sections.insert(key.clone());
                    }
                    cx.notify();
                });
            });
        let muted_line = |text: String| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
        };

        // Two columns: the chevron alone in a gutter, and every line of text
        // in one column beside it, so the name lines up with the lines
        // under it rather than sitting indented past the chevron.
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_1()
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
                            .text_color(detail::tone_color(container.state_tone, cx))
                            .child(container.state.clone()),
                    ),
            )
            .child(div().text_sm().min_w_0().child(container.image.clone()))
            .when(container.restart_count > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(detail::tone_color(BadgeTone::Warning, cx))
                        .child(format!("{} restarts", container.restart_count)),
                )
            })
            .when(!container.ports.is_empty(), |this| {
                this.child(muted_line(format!("Ports: {}", container.ports.join(", "))))
            })
            .when(!container.requests.is_empty(), |this| {
                this.child(muted_line(format!(
                    "Requests: {}",
                    container.requests.join(", ")
                )))
            })
            .when(!container.limits.is_empty(), |this| {
                this.child(muted_line(format!(
                    "Limits: {}",
                    container.limits.join(", ")
                )))
            })
            .when(!container.env_sources.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x_1()
                        .text_sm()
                        .child(div().text_color(theme.muted_foreground).child("Env from:"))
                        .child(self.render_references(
                            format!("env-{}", container.name),
                            &container.env_sources,
                            true,
                            cx,
                        )),
                )
            })
            .when(open, |this| {
                this.child(render_container_detail(
                    &container.name,
                    &container.detail,
                    cx,
                ))
            });

        div()
            .flex()
            .items_start()
            .gap_1()
            .p_2()
            .rounded_md()
            .bg(crate::ui::style::surface_card(cx))
            .border_1()
            .border_color(theme.border)
            .child(div().flex_none().child(toggle))
            .child(body)
            .into_any_element()
    }
}

/// The expanded block: one labelled group per field the container sets,
/// left out when it sets none of it.
fn render_container_detail(name: &str, detail: &ContainerDetail, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    let group = |label: &'static str, body: AnyElement| {
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child(label),
            )
            .child(body)
    };
    let lines = |lines: &[String]| {
        div()
            .flex()
            .flex_col()
            .font_family(mono.clone())
            .text_sm()
            .children(lines.iter().map(|line| div().child(line.clone())))
            .into_any_element()
    };
    let is_empty = detail == &ContainerDetail::default();

    div()
        .id(container_detail_id(name))
        .flex()
        .flex_col()
        .gap_2()
        .pt_2()
        .border_t_1()
        .border_color(theme.border)
        .when(is_empty, |this| {
            this.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("No env, mounts, probes, command or security context set."),
            )
        })
        .when(!detail.command.is_empty(), |this| {
            this.child(group("Command", lines(&[detail.command.join(" ")])))
        })
        .when(!detail.args.is_empty(), |this| {
            this.child(group("Args", lines(&detail.args)))
        })
        .when(!detail.env.is_empty(), |this| {
            this.child(group(
                "Environment",
                div()
                    .flex()
                    .flex_col()
                    .text_sm()
                    .children(detail.env.iter().map(|var| {
                        let row = div()
                            .flex()
                            .flex_wrap()
                            .gap_x_1()
                            .child(div().font_family(mono.clone()).child(var.name.clone()));
                        // A reference reads as a muted description, never as
                        // a value - see `EnvValue`.
                        match &var.value {
                            EnvValue::Literal(value) => row
                                .child(div().font_family(mono.clone()).child(format!("= {value}"))),
                            EnvValue::Reference(source) => row.child(
                                div()
                                    .text_color(theme.muted_foreground)
                                    .child(source.clone()),
                            ),
                            // Danger-tinted, as a Secret's Show button is:
                            // the value behind it is sensitive.
                            EnvValue::SecretReference(source) => {
                                row.child(div().text_color(theme.danger).child(source.clone()))
                            }
                        }
                    }))
                    .into_any_element(),
            ))
        })
        .when(!detail.volume_mounts.is_empty(), |this| {
            this.child(group("Volume Mounts", lines(&detail.volume_mounts)))
        })
        .when(!detail.probes.is_empty(), |this| {
            this.child(group("Probes", lines(&detail.probes)))
        })
        .when(!detail.security_context.is_empty(), |this| {
            this.child(group(
                "Security Context",
                detail::chips(&detail.security_context, cx),
            ))
        })
        .test_support()
        .into_any_element()
}
