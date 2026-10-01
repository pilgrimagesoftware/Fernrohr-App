//! Drawing the object panel: the header with its shortcut hints, the
//! stacked sections, the events, and the YAML view.

use super::commands::{PANEL_KEY_CONTEXT, TOGGLE_VIEW_KEY, ToggleObjectView};
use super::fetch::ObjectDetailState;
use super::model::{FieldValue, ObjectField};
use super::panel::ObjectDetailPanel;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::events;
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::{detail, link, panel_title};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use jiff::Timestamp;

impl ObjectDetailPanel {
    fn render_field(&self, section: &str, field: &ObjectField, cx: &Context<Self>) -> AnyElement {
        let value = match &field.value {
            FieldValue::Text(text) => div().child(text.clone()).into_any_element(),
            FieldValue::References { targets, qualified } => {
                let qualified = *qualified;
                link::references(
                    format!("{section}/{}", field.label),
                    targets,
                    move |target: &ObjectRef| {
                        if qualified {
                            target.qualified_name()
                        } else {
                            target.name.clone()
                        }
                    },
                    &self.scope.context_name,
                    self.kinds(cx),
                    cx,
                )
            }
            FieldValue::Chips(chips) => detail::chips(chips, cx),
            FieldValue::Badges(badges) => {
                detail::badges(badges.iter().map(|(text, tone)| (text.as_str(), *tone)), cx)
            }
            FieldValue::Lines(lines) => detail::lines(lines),
            FieldValue::KeyValues(pairs) => detail::key_values(pairs, cx),
            FieldValue::SecretKeys { secret, keys } => div()
                .flex()
                .flex_col()
                .gap_1()
                .children(keys.iter().enumerate().map(|(index, (key, size))| {
                    let this = cx.weak_entity();
                    let (target, owned_key) = (secret.clone(), key.clone());
                    detail::secret_key_row(
                        key,
                        *size,
                        self.revealed.get(key),
                        ElementId::NamedInteger("object-reveal".into(), index as u64),
                        ElementId::NamedInteger("object-revealed".into(), index as u64),
                        move |_window, cx| {
                            let _ = this.update(cx, |this: &mut Self, cx| {
                                this.toggle_reveal(target.clone(), owned_key.clone(), cx)
                            });
                        },
                        cx,
                    )
                }))
                .into_any_element(),
        };
        detail::row(field.label.clone(), value, cx)
    }

    fn render_structured(&self, cx: &Context<Self>) -> AnyElement {
        let events = match &self.state {
            ObjectDetailState::Loaded(_, Ok(events)) => {
                Ok(events::summarize(events, Timestamp::now()))
            }
            ObjectDetailState::Loaded(_, Err(reason)) => Err(reason.clone()),
            _ => Ok(Vec::new()),
        };
        div()
            .flex()
            .flex_col()
            .children(self.sections(Timestamp::now()).iter().map(|section| {
                div()
                    .flex()
                    .flex_col()
                    .child(detail::section_heading(section.title.clone(), cx))
                    .child(detail::striped(
                        section
                            .fields
                            .iter()
                            .map(|field| self.render_field(&section.title, field, cx)),
                        cx,
                    ))
            }))
            .child(detail::section_heading("Events", cx))
            .child(div().pt_1().child(detail::events(&events, cx)))
            .into_any_element()
    }

    fn render_yaml(&self, cx: &App) -> AnyElement {
        let Some(yaml) = self.yaml() else {
            return div().into_any_element();
        };
        div()
            .size_full()
            .font_family(cx.theme().mono_font_family.clone())
            .whitespace_nowrap()
            .child(yaml)
            .into_any_element()
    }

    /// The kind as a reader says it, lower case: "this configmap".
    fn kind_noun(&self) -> String {
        self.target.kind.gvk.kind.to_lowercase()
    }
}

impl Render for ObjectDetailPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.state {
            ObjectDetailState::Loading => div()
                .size_full()
                .p_3()
                .child(format!("Loading {}...", self.kind_noun()))
                .into_any_element(),
            ObjectDetailState::NotFound => div()
                .id("object-not-found")
                .size_full()
                .p_3()
                .child(format!("This {} doesn't exist.", self.kind_noun()))
                .test_support()
                .into_any_element(),
            ObjectDetailState::Failed { message, detail } => panel_title::error_content(
                format!("Could not read {}: {message}", self.kind_noun()),
                Some(detail.clone()),
                cx,
            )
            .into_any_element(),
            ObjectDetailState::Loaded(_, _) => match self.viewing {
                DetailView::Structured => div()
                    .size_full()
                    .p_3()
                    .overflow_y_scrollbar()
                    .child(self.render_structured(cx))
                    .into_any_element(),
                DetailView::Yaml => div()
                    .size_full()
                    .p_3()
                    .overflow_scrollbar()
                    .child(self.render_yaml(cx))
                    .into_any_element(),
            },
        };

        let window_contexts = crate::util::shell::window_context_count(window, cx);
        let yaml = self.viewing == DetailView::Yaml;
        let has_links = !self.followable(cx).is_empty();
        let toggle_key =
            Kbd::binding_for_action(&ToggleObjectView, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| {
                    Kbd::new(Keystroke::parse(TOGGLE_VIEW_KEY).expect("valid keybinding"))
                });
        let hint = |key: Kbd, label: &'static str| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .whitespace_nowrap()
                .child(key)
                .child(label)
        };
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .p_2()
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w(rems(8.))
                    .child(panel_title::item_heading(
                        self.target.name.clone(),
                        panel_title::heading_context(&self.scope, window_contexts),
                        cx.theme().muted_foreground,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .min_w_0()
                    .gap_x_3()
                    .gap_y_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .when(has_links, |this| {
                        this.child(
                            hint(link::go_to_key(window), "Go to…")
                                .id("go-to-hint")
                                .test_support(),
                        )
                    })
                    .child(hint(
                        toggle_key,
                        if yaml { "Show fields" } else { "Show YAML" },
                    )),
            );

        div()
            .size_full()
            .key_context(key_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_toggle_view))
            .on_action(cx.listener(Self::on_action_hide_secret_values))
            .on_action(cx.listener(Self::on_action_go_to))
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(content))
    }
}

/// The panel's own key context plus the shared one `links.go_to` is gated to.
fn key_context() -> KeyContext {
    let mut context = KeyContext::default();
    context.add(PANEL_KEY_CONTEXT);
    context.add(link::LINKS_KEY_CONTEXT);
    context
}
