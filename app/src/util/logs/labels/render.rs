//! Drawing a label-following panel's bar: the workload it follows, or a
//! typed panel's selector field; how many pods and containers match; why a
//! typed selector wasn't applied; and the shared stream controls.

use super::*;
use crate::consts::LABEL_LOGS_MAX_STREAMS;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;

/// A typed panel's selector field, for tests.
pub(in crate::util::logs) const SELECTOR_INPUT: &str = "logs-selector";
/// What a typed panel's field shows while it's empty.
const SELECTOR_PLACEHOLDER: &str = "Label selector, such as app=web, tier in (front)";

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
        if following.source != LabelLogs::Typed {
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
        if self
            .labels
            .as_ref()
            .is_some_and(|following| following.source == LabelLogs::Typed)
        {
            context.add(super::super::commands::TYPED_LABELS_KEY_CONTEXT);
        }
        context
    }

    /// Moves focus to a typed panel's selector field - its `/`, and the
    /// palette's Follow Labels when the panel is already open.
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
        let following = self.labels.as_ref()?;
        let space = crate::ui::space::spacing(cx);
        let muted = cx.theme().muted_foreground;
        let what = match (&following.source, input) {
            (_, Some(input)) => div()
                .debug_selector(|| SELECTOR_INPUT.into())
                .child(Input::new(&input).small())
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
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .truncate()
                        .child(workload.selector.clone()),
                )
                .into_any_element(),
            (LabelLogs::Typed, None) => div().into_any_element(),
        };
        let matched = following.matched;
        let counts = following.selector.is_some().then(|| {
            let mut counts = format!(
                "{} · {}",
                plural(matched.pods, "pod"),
                plural(matched.containers, "container")
            );
            if matched.containers > LABEL_LOGS_MAX_STREAMS {
                counts.push_str(&format!(" · streaming {LABEL_LOGS_MAX_STREAMS}"));
            }
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(muted)
                .whitespace_nowrap()
                .child(counts)
        });
        let error = following.error.clone().map(|error| {
            div()
                .px(space.panel_inset)
                .pb(space.control_gap)
                .text_sm()
                .text_color(crate::ui::style::status(crate::ui::style::Tone::Bad, cx))
                .child(error)
        });
        let view = self.view.read(cx);
        let line_count = view.lines().len();
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
                        .children(counts)
                        .when(streaming, |this| {
                            this.child(super::super::render::stream_controls(
                                line_count,
                                following_lines,
                                cx,
                            ))
                        }),
                )
                .children(error)
                .into_any_element(),
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
        Some(match (following.matched.pods, &following.source) {
            (0, LabelLogs::Workload(workload)) => format!(
                "No pods in {} match {} {}'s selector, {selector}.",
                workload.namespace, workload.kind, workload.name
            ),
            (0, LabelLogs::Typed) => format!("No pods match {selector}."),
            (_, _) if following.matched.containers == 0 => {
                "The matching pods' containers haven't started yet.".into()
            }
            _ => "Waiting for log lines…".into(),
        })
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}
