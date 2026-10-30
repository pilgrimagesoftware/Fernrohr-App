//! Drawing a label-following panel's bar: the workload it follows, or a
//! typed panel's selector field; how many pods and containers match; why a
//! typed selector wasn't applied; and the shared stream controls.

use super::*;
use crate::consts::LABEL_LOGS_MAX_STREAMS;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;

/// A typed panel's selector field, for tests.
pub(in crate::util) const SELECTOR_INPUT: &str = "logs-selector";
/// A typed panel's reset button (#247), for tests.
pub(in crate::util) const RESET_BUTTON: &str = "logs-label-reset";
/// The reset button's tooltip.
pub(in crate::util) const RESET_TOOLTIP: &str = "Clear selector and logs";
/// The container filter's button, for tests.
pub(in crate::util::logs) const CONTAINER_FILTER: &str = "logs-label-container";
/// The container filter's label while it streams every container.
const ALL_CONTAINERS: &str = "All containers";
/// A workload's selector, shown readably with its own text in a tooltip, for
/// tests.
pub(in crate::util) const WORKLOAD_SELECTOR: &str = "logs-workload-selector";
/// What a typed panel's field shows while it's empty.
const SELECTOR_PLACEHOLDER: &str = "Pod labels to match, such as app=web";
/// The typed field's tooltip: the forms a selector's requirements take (#244).
pub(in crate::util) const SELECTOR_HELP: &str = "Pods whose labels match every requirement. \
     Separate requirements with commas.\n\
     key=value: key is value\n\
     key!=value: key isn't value\n\
     key in (a,b): key is a or b\n\
     key notin (a,b): key is neither a nor b\n\
     key: has the label key\n\
     !key: doesn't have the label key";

impl LogsPanel {
    /// A typed panel's selector field, made the first time the panel renders:
    /// Enter applies what's in it. A new panel with no selector yet starts
    /// with the field focused, since there's nothing else to do in it.
    fn selector_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<InputState>> {
        let following = self.labels.as_mut()?;
        if !following.source.is_typed() {
            return None;
        }
        if let Some(input) = &following.input {
            return Some(input.clone());
        }
        let text = following
            .selector
            .as_ref()
            .map(|(text, _)| text.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(SELECTOR_PLACEHOLDER);
            input.set_value(text, window, cx);
            input
        });
        cx.subscribe(&input, |this: &mut Self, input, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = input.read(cx).value().to_string();
                this.apply_selector_text(&text, cx);
            }
        })
        .detach();
        if following.selector.is_none() {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
        following.input = Some(input.clone());
        Some(input)
    }

    /// The panel's key context: a typed label panel's adds its own, where the
    /// key focusing its field is bound.
    pub(in crate::util::logs) fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.add(PANEL_KEY_CONTEXT);
        if self.labels.is_some() {
            context.add(super::super::commands::LABEL_LOGS_KEY_CONTEXT);
        }
        if self
            .labels
            .as_ref()
            .is_some_and(|following| following.source.is_typed())
        {
            context.add(super::super::commands::TYPED_LABELS_KEY_CONTEXT);
        }
        context
    }

    /// Moves focus to a typed panel's selector field - its `/`, and the
    /// palette's or menu's Open Logs by Label… when the panel is already open.
    pub fn focus_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.selector_input(window, cx) {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    /// The bar a label-following panel shows in place of a pod's; `None` for a
    /// pod's logs.
    pub(in crate::util::logs) fn label_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let input = self.selector_input(window, cx);
        let reset = self
            .is_typed_labels()
            .then(|| self.reset_control(window, cx));
        let context_control = self.context_control(window, cx);
        let scope_picker = self.scope_picker(window, cx);
        let following = self.labels.as_ref()?;
        let space = crate::ui::space::spacing(cx);
        let muted = cx.theme().muted_foreground;
        let what = match (&following.source, input) {
            (_, Some(input)) => crate::ui::icon_tooltip::with_tooltip(
                SELECTOR_INPUT,
                SELECTOR_HELP,
                div()
                    .debug_selector(|| SELECTOR_INPUT.into())
                    // The clear button empties the field's text only; Enter
                    // applies what's in it, and ctrl-l resets the panel (#247).
                    .child(crate::ui::text_field::text_field(&input, cx).small()),
            )
            .into_any_element(),
            (LabelLogs::Workload(workload), None) => div()
                .flex()
                .items_center()
                .gap(space.control_gap)
                .min_w_0()
                .child(panel_title::item_heading(
                    format!("{} {}", workload.kind, workload.name),
                    panel_title::heading_context(
                        &self.scope,
                        crate::util::shell::window_context_count(window, cx),
                    ),
                    muted,
                ))
                // Readable - `tier: front` for `tier in (front)` (#244) - with
                // the selector's own text in its tooltip.
                .child(
                    crate::ui::icon_tooltip::with_tooltip_text(
                        WORKLOAD_SELECTOR,
                        format!("Label selector: {}", workload.selector),
                        div()
                            .debug_selector(|| WORKLOAD_SELECTOR.into())
                            .text_sm()
                            .text_color(muted)
                            .truncate()
                            .child(crate::k8s::label_selector::readable_text(
                                &workload.selector,
                            )),
                    )
                    .min_w_0(),
                )
                .into_any_element(),
            (LabelLogs::Typed(_), None) => div().into_any_element(),
        };
        let matched = following.matched;
        let counts = following.selector.is_some().then(|| {
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(muted)
                .whitespace_nowrap()
                .child(counts_text(matched))
        });
        let error = following.error.clone().map(|error| {
            div()
                .px(space.panel_inset)
                .pb(space.control_gap)
                .text_sm()
                .text_color(crate::ui::style::status(crate::ui::style::Tone::Bad, cx))
                .child(error)
        });
        let container_filter = following
            .selector
            .is_some()
            .then(|| self.container_filter(window, cx));
        let view = self.view.read(cx);
        let line_count = view.row_count();
        let following_lines = view.follow_state() == FollowState::Following;
        let streaming = following.selector.is_some();
        Some(
            div()
                .flex()
                .flex_col()
                .bg(crate::ui::style::surface_raised(cx))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(space.control_gap)
                        .px(space.panel_inset)
                        .py(space.control_gap)
                        .child(div().flex_1().min_w_0().child(what))
                        .children(reset)
                        .children(context_control)
                        .children(scope_picker)
                        .children(counts)
                        .children(container_filter)
                        .when(streaming, |this| {
                            this.child(super::super::render::stream_controls(
                                line_count,
                                following_lines,
                                window,
                                cx,
                            ))
                        }),
                )
                .children(error)
                .into_any_element(),
        )
    }

    /// A typed panel's reset (#247): its key, then an icon button that clears
    /// the selector and the lines and stops every stream.
    fn reset_control(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement + use<> {
        let key = Kbd::binding_for_action(
            &super::super::commands::ResetLabelLogs,
            Some(PANEL_KEY_CONTEXT),
            window,
        )
        .unwrap_or_else(|| {
            Kbd::new(
                Keystroke::parse(super::super::commands::RESET_LABELS_KEY)
                    .expect("valid keybinding"),
            )
        });
        let this = cx.weak_entity();
        div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(key)
            .child(crate::ui::icon_tooltip::with_tooltip(
                RESET_BUTTON,
                RESET_TOOLTIP,
                Button::new(RESET_BUTTON)
                    .icon(gpui_kit::assets::IconName::Delete)
                    .xsmall()
                    .ghost()
                    .tab_stop(false)
                    .on_click(move |_event, window, cx| {
                        let _ =
                            this.update(cx, |this: &mut Self, cx| this.reset_labels(window, cx));
                    }),
            ))
    }

    /// The container filter: its key, then a button reading the filter that
    /// drops down all containers and each name the matched pods have.
    fn container_filter(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement + use<> {
        let following = self.labels.as_ref();
        let current = following.and_then(|following| following.container.clone());
        let mut names = following
            .map(|following| following.containers.clone())
            .unwrap_or_default();
        // A restored filter shows even before the pods it names are seen.
        if let Some(current) = &current
            && !names.contains(current)
        {
            names.push(current.clone());
        }
        let key = Kbd::binding_for_action(&NextLabelContainer, Some(PANEL_KEY_CONTEXT), window)
            .unwrap_or_else(|| {
                Kbd::new(
                    Keystroke::parse(super::super::commands::CONTAINER_FILTER_KEY)
                        .expect("valid keybinding"),
                )
            });
        let this = cx.weak_entity();
        div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(key)
            .child(
                Button::new(CONTAINER_FILTER)
                    .label(current.clone().unwrap_or_else(|| ALL_CONTAINERS.into()))
                    .icon(gpui_kit::assets::IconName::ChevronDown)
                    .xsmall()
                    .ghost()
                    .tab_stop(false)
                    .tooltip("Which containers stream from each pod")
                    .dropdown_menu(move |menu, _window, _cx| {
                        let choices = std::iter::once(None).chain(names.iter().cloned().map(Some));
                        choices.fold(menu, |menu, choice| {
                            let this = this.clone();
                            let label = choice.clone().unwrap_or_else(|| ALL_CONTAINERS.into());
                            menu.item(
                                gpui_kit::component::menu::PopupMenuItem::new(label)
                                    .checked(choice == current)
                                    .on_click(move |_event, _window, cx| {
                                        let _ = this.update(cx, |this: &mut Self, cx| {
                                            this.set_container_filter(choice.clone(), cx);
                                        });
                                    }),
                            )
                        })
                    }),
            )
    }

    /// What a label-following panel shows in place of lines while it has none
    /// to show: no selector yet, or none of the pods matching.
    pub(in crate::util::logs) fn label_placeholder(&self, cx: &App) -> Option<String> {
        let following = self.labels.as_ref()?;
        let Some((selector, _)) = &following.selector else {
            return Some("Type a label selector, such as app=web, and press Enter.".into());
        };
        let synced = following
            .pods
            .as_ref()
            .is_some_and(|table| table.read(cx).synced());
        if !synced || !self.view.read(cx).lines().is_empty() {
            return None;
        }
        Some(no_lines_message(following, selector))
    }
}

/// Why a label panel streaming `selector` shows no lines yet: no pod matches,
/// none has the container the filter names, their containers haven't
/// started, or the lines just haven't come.
pub(super) fn no_lines_message(following: &Following, selector: &str) -> String {
    match (following.matched.pods, &following.source) {
        (0, LabelLogs::Workload(workload)) => format!(
            "No pods in {} match {} {}'s selector, {selector}.",
            workload.namespace, workload.kind, workload.name
        ),
        (0, LabelLogs::Typed(_)) => format!("No pods match {selector}."),
        _ if following.matched.containers > 0 => "Waiting for log lines…".into(),
        _ => match &following.container {
            Some(name) if !following.containers.contains(name) => {
                format!("No matching pod has a container named {name}.")
            }
            _ => "The matching pods' containers haven't started yet.".into(),
        },
    }
}

/// What the bar says the selector matched: pods and containers, and - past
/// the cap - how many of those containers stream.
pub(super) fn counts_text(matched: Matched) -> String {
    let mut counts = format!(
        "{} · {}",
        plural(matched.pods, "pod"),
        plural(matched.containers, "container")
    );
    if matched.containers > LABEL_LOGS_MAX_STREAMS {
        counts.push_str(&format!(" · streaming {LABEL_LOGS_MAX_STREAMS}"));
    }
    counts
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}
