//! Drawing the object panel: the header with its shortcut hints, the
//! stacked sections, the events, and the YAML view.

use super::commands::{
    DELETE_KEY, DeleteObject, PANEL_KEY_CONTEXT, TOGGLE_VIEW_KEY, ToggleObjectView,
};
use super::fetch::ObjectDetailState;
use super::model::{FieldValue, ObjectField};
use super::panel::ObjectDetailPanel;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::events;
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::{detail, link, panel_title};
use gpui_kit::base::FocusTrapElement as _;
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
            FieldValue::Status { text, tone } => div()
                .debug_selector(|| format!("object-status-{section}/{}", field.label))
                .text_color(crate::ui::style::status(*tone, cx))
                .child(text.clone())
                .into_any_element(),
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
            FieldValue::Metadata(pairs) => {
                detail::metadata_chips(&format!("{section}/{}", field.label), pairs)
            }
            FieldValue::Badges(badges) => {
                detail::badges(badges.iter().map(|(text, tone)| (text.as_str(), *tone)), cx)
            }
            FieldValue::Lines(lines) => detail::lines(lines),
            FieldValue::Urls(urls) => detail::urls(&format!("{section}/{}", field.label), urls),
            FieldValue::KeyValues(pairs) => {
                detail::key_values(&format!("{section}/{}", field.label), pairs, cx)
            }
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

    /// The manifest, folding and scrolling both ways (`ui::yaml_view`).
    fn render_yaml(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(yaml) = self.yaml() else {
            return div().into_any_element();
        };
        let this = cx.weak_entity();
        let on_toggle: crate::ui::yaml_view::OnToggle = std::rc::Rc::new(move |line, _, cx| {
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.yaml_view.toggle(line);
                cx.notify();
            });
        });
        self.yaml_view.element(&yaml, on_toggle, cx)
    }

    fn on_action_fold_all(
        &mut self,
        _: &crate::ui::yaml_view::FoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(yaml) = self.yaml() {
            self.yaml_view.fold_all(&yaml);
            cx.notify();
        }
    }

    /// Copy Resource Name: the object's name to the clipboard.
    fn on_action_copy_name(
        &mut self,
        _: &crate::ui::copy::CopyResourceName,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::ui::copy::copy_text(&self.target.name, cx);
    }

    /// Copy YAML (#154): the whole manifest to the clipboard, folded blocks
    /// included.
    fn on_action_copy_yaml(
        &mut self,
        _: &crate::ui::yaml_view::CopyYaml,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(yaml) = self.yaml() {
            crate::ui::copy::copy_text(&yaml, cx);
        }
    }

    fn on_action_unfold_all(
        &mut self,
        _: &crate::ui::yaml_view::UnfoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.yaml_view.unfold_all();
        cx.notify();
    }

    /// The kind as a reader says it, lower case: "this configmap".
    fn kind_noun(&self) -> String {
        self.target.kind.gvk.kind.to_lowercase()
    }
}

impl Render for ObjectDetailPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        self.start_pending_edit(window, cx);
        let editing = self.render_edit(window, cx);
        let content = match &self.state {
            _ if editing.is_some() => editing.unwrap_or_else(|| div().into_any_element()),
            ObjectDetailState::Loading => div()
                .size_full()
                .p(space.panel_inset)
                .child(format!("Loading {}...", self.kind_noun()))
                .into_any_element(),
            ObjectDetailState::NotFound => div()
                .id("object-not-found")
                .size_full()
                .p(space.panel_inset)
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
                    .p(space.panel_inset)
                    .overflow_y_scrollbar()
                    .child(self.render_structured(cx))
                    .into_any_element(),
                // The view scrolls itself, both ways.
                DetailView::Yaml => div()
                    .size_full()
                    .p(space.panel_inset)
                    .child(self.render_yaml(cx))
                    .into_any_element(),
            },
        };

        let window_contexts = crate::util::shell::window_context_count(window, cx);
        let yaml = self.viewing == DetailView::Yaml;
        let has_links = !self.followable(cx).is_empty();
        let deletable = self.deletable(cx);
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
        let logs_hint = self.logs_hint(window, cx);
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(space.control_gap)
            .px(space.panel_inset)
            .py(space.control_gap)
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w(rems(8.))
                    .child(panel_title::with_header_icon(
                        crate::ui::icon::for_kind(
                            &self.target.kind.gvk.group,
                            &self.target.kind.gvk.kind,
                        ),
                        panel_title::item_heading(
                            self.target.name.clone(),
                            panel_title::heading_context(&self.scope, window_contexts),
                            cx.theme().muted_foreground,
                        ),
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .min_w_0()
                    .gap_x(space.control_gap)
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
                    .children(logs_hint)
                    .when(deletable, |this| {
                        this.child(hint(
                            Kbd::binding_for_action(
                                &DeleteObject,
                                Some(super::delete::DELETABLE_KEY_CONTEXT),
                                window,
                            )
                            .unwrap_or_else(|| {
                                Kbd::new(Keystroke::parse(DELETE_KEY).expect("valid keybinding"))
                            }),
                            "Delete",
                        ))
                    })
                    .child(hint(
                        toggle_key,
                        if yaml { "Show fields" } else { "Show YAML" },
                    )),
            );

        div()
            .size_full()
            .key_context(key_context(
                self.edit.is_some(),
                deletable,
                self.verbs(cx).patch,
                self.pod_logs_workload().is_some(),
            ))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_toggle_view))
            .on_action(cx.listener(Self::on_action_fold_all))
            .on_action(cx.listener(Self::on_action_copy_name))
            .on_action(cx.listener(Self::on_action_unfold_all))
            .on_action(cx.listener(Self::on_action_copy_yaml))
            .on_action(cx.listener(Self::on_action_hide_secret_values))
            .on_action(cx.listener(Self::on_action_go_to))
            .capture_action(cx.listener(Self::capture_editor_escape))
            .on_action(cx.listener(Self::on_action_edit))
            .on_action(cx.listener(Self::on_action_save_edit))
            .on_action(cx.listener(Self::on_action_cancel_edit))
            .on_action(cx.listener(Self::on_action_delete_object))
            .on_action(cx.listener(Self::on_action_show_logs))
            .flex()
            .flex_col()
            .child(header)
            .children(self.render_edit_notice(cx))
            .children(self.render_refusal(cx))
            .child(crate::ui::detail::lifecycle::body(
                content,
                self.lifecycle.as_ref(),
                &self.kind_noun(),
                cx,
            ))
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("object-detail-panel-tab-trap", &self.focus_handle)
    }
}

/// The panel's own key context plus the shared one `links.go_to` is gated to -
/// and, while `editing`, the edit's own; while `deletable`, Delete's; while
/// the kind is `patchable` and no edit is open, Edit's; while the object
/// `selects_pods`, Logs'.
fn key_context(editing: bool, deletable: bool, patchable: bool, selects_pods: bool) -> KeyContext {
    let mut context = KeyContext::default();
    context.add(PANEL_KEY_CONTEXT);
    if selects_pods {
        context.add(super::commands::POD_SELECTING_KEY_CONTEXT);
    }
    if deletable {
        context.add(super::delete::DELETABLE_KEY_CONTEXT);
    }
    if patchable && !editing {
        context.add(super::commands::EDITABLE_KEY_CONTEXT);
    }
    if editing {
        context.add(super::commands::EDIT_KEY_CONTEXT);
    } else {
        // `g` (go to) is a links command, not the panel's: leave its context
        // out while editing, so `g` typed into the YAML types.
        context.add(link::LINKS_KEY_CONTEXT);
    }
    context
}
