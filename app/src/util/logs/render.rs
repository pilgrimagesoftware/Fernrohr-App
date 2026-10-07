//! Drawing the panel: the log line list (or the terminal-state message), the
//! container picker, and the control bar (jump to top/bottom, follow toggle).

use super::*;
use crate::ui::typography::TypeRole as _;
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::prelude::FluentBuilder as _;

/// The control bar's Previous toggle.
pub(super) const PREVIOUS_TOGGLE_ID: &str = "logs-previous";
/// The log lines' list, for tests.
pub(super) const LINES: &str = "logs-lines";

impl Render for LogsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A label-following panel's bar, in place of a pod's (#150).
        let label_bar = self.label_bar(window, cx);
        let view = self.view.read(cx);
        let following = view.follow_state() == FollowState::Following;
        let content = if let Some(message) = view.terminal_message() {
            let message = message.to_string();
            let detail = view.terminal_detail().map(str::to_string);
            panel_title::error_content(message, detail, cx).into_any_element()
        } else if let Some(message) = self.label_placeholder(cx) {
            div()
                .size_full()
                .p(crate::ui::space::spacing(cx).panel_inset)
                .child(message)
                .into_any_element()
        } else if self.current.is_none() && self.labels.is_none() {
            div()
                .size_full()
                .p(crate::ui::space::spacing(cx).panel_inset)
                .child("Click a pod to view its logs.")
                .into_any_element()
        } else {
            self.lines_list(view.lines(), cx)
        };

        // A container picker, not a namespace one: this panel streams one
        // pod's logs, already chosen by clicking that pod - there is no
        // namespace left to scope, only which of its containers to read.
        let container_picker = view.needs_container_picker().then(|| {
            let containers = view.containers().to_vec();
            let selected = view.selected_container().to_string();
            let this = cx.weak_entity();
            Button::new("logs-container")
                .label(selected.clone())
                .icon(gpui_kit::assets::IconName::ChevronDown)
                .xsmall()
                .ghost()
                .tab_stop(false)
                .tooltip("Container")
                .dropdown_menu(move |menu, _window, _cx| {
                    let mut menu = menu;
                    for container in &containers {
                        let this = this.clone();
                        let container = container.clone();
                        let checked = container == selected;
                        menu = menu.item(
                            gpui_kit::component::menu::PopupMenuItem::new(container.clone())
                                .checked(checked)
                                .on_click(move |_event, _window, cx| {
                                    let _ = this.update(cx, |this: &mut Self, cx| {
                                        this.switch_container(container.clone(), cx);
                                    });
                                }),
                        );
                    }
                    menu
                })
        });

        let heading = self.current.as_ref().map(|(_, pod, container)| {
            panel_title::item_heading(
                format!("{pod} / {container}"),
                panel_title::heading_context(
                    &self.scope,
                    crate::util::shell::window_context_count(window, cx),
                ),
                cx.theme().muted_foreground,
            )
        });
        // The bar shows once a pod is picked - in a terminal state too, so the
        // Previous toggle is there to switch back from "no previous instance".
        let streaming = view.terminal_message().is_none();
        let previous = self.previous;
        let this_previous = cx.weak_entity();
        let previous_key = gpui_kit::component::kbd::Kbd::binding_for_action(
            &TogglePreviousLogs,
            Some(PANEL_KEY_CONTEXT),
            window,
        )
        .unwrap_or_else(|| {
            gpui_kit::component::kbd::Kbd::new(
                Keystroke::parse(PREVIOUS_KEY).expect("valid keybinding"),
            )
        });
        let control_bar = self.current.is_some().then(|| {
            let line_count = view.lines().len();
            div()
                .flex()
                .items_center()
                .gap(crate::ui::space::spacing(cx).control_gap)
                .px(crate::ui::space::spacing(cx).panel_inset)
                .py(crate::ui::space::spacing(cx).control_gap)
                .bg(crate::ui::style::surface_raised(cx))
                .border_b_1()
                .border_color(cx.theme().border)
                // "pod / container" (plus the context in a multi-context window) on
                // the left; the controls take the rest of the row on the right.
                .child(div().flex_1().min_w_0().children(heading))
                .children(container_picker)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(previous_key)
                        .child(
                            Button::new(PREVIOUS_TOGGLE_ID)
                                .label("Previous")
                                .xsmall()
                                .ghost()
                                .toggled(previous)
                                .tooltip("The container's previous instance's logs")
                                .on_click(move |_event, _window, cx| {
                                    let _ = this_previous.update(cx, |this: &mut Self, cx| {
                                        this.toggle_previous(cx);
                                    });
                                }),
                        ),
                )
                .when(streaming, |this| {
                    this.child(stream_controls(line_count, following, cx))
                })
        });
        let control_bar = match label_bar {
            Some(bar) => Some(bar),
            None => control_bar.map(IntoElement::into_any_element),
        };

        div()
            .size_full()
            .key_context(self.key_context())
            // Tracked so a click focuses the panel, which is what lights its
            // tab's focus underline.
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &TogglePreviousLogs, _window, cx| {
                this.toggle_previous(cx);
            }))
            .on_action(cx.listener(|this, _: &FocusLabelSelector, window, cx| {
                this.focus_selector(window, cx);
            }))
            .flex()
            .flex_col()
            .children(control_bar)
            .child(div().flex_1().min_h_0().child(content))
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("logs-panel-tab-trap", &self.focus_handle)
            .into_any_element()
    }
}

impl LogsPanel {
    /// The log lines, each selectable, in a list that scrolls itself and
    /// follows the newest line while following.
    pub(super) fn lines_list(&self, lines: &[String], cx: &Context<Self>) -> AnyElement {
        let lines = Rc::new(lines.to_vec());
        let line_count = lines.len();
        div()
            .debug_selector(|| LINES.into())
            .on_scroll_wheel(cx.listener(Self::on_lines_scrolled))
            .size_full()
            .p(crate::ui::space::spacing(cx).panel_inset)
            .code_font(cx)
            .overflow_x_scrollbar()
            .child(
                uniform_list(
                    "logs-panel-lines",
                    line_count,
                    move |range, _window, _cx| {
                        range
                            .map(|ix| {
                                // Each line is selectable text in the window's
                                // selection, ordered by its line number, so a drag
                                // across lines selects and copies them in reading
                                // order (#151). One participant per line: runs
                                // sharing a handle keep only the last one drawn.
                                div()
                                    .whitespace_nowrap()
                                    .child(
                                        gpui_kit::base::SelectableText::new(
                                            ("logs-line", ix),
                                            lines[ix].clone(),
                                        )
                                        .document_order(ix as u64),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    },
                )
                .track_scroll(&self.scroll_handle)
                .size_full(),
            )
            .into_any_element()
    }
}

/// Jump to top, jump to bottom, and the Follow toggle - for a pod's stream and
/// a label panel's alike.
pub(super) fn stream_controls(
    line_count: usize,
    following: bool,
    cx: &Context<LogsPanel>,
) -> impl IntoElement + use<> {
    let this_top = cx.weak_entity();
    let this_bottom = cx.weak_entity();
    let this_follow = cx.weak_entity();
    div()
        .flex()
        .items_center()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .child(
            Button::new("logs-jump-top")
                .icon(gpui_kit::assets::IconName::ArrowUp)
                .xsmall()
                .ghost()
                .tab_stop(false)
                .tooltip("Jump to top")
                .on_click(move |_event, _window, cx| {
                    let _ = this_top.update(cx, |this: &mut LogsPanel, cx| {
                        this.view.update(cx, |view, _| view.scroll_up());
                        this.scroll_handle
                            .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
                        cx.notify();
                    });
                }),
        )
        .child(
            Button::new("logs-jump-bottom")
                .icon(gpui_kit::assets::IconName::ArrowDown)
                .xsmall()
                .ghost()
                .tab_stop(false)
                .tooltip("Jump to bottom")
                .on_click(move |_event, _window, cx| {
                    let _ = this_bottom.update(cx, |this: &mut LogsPanel, cx| {
                        this.view.update(cx, |view, _| view.scroll_to_bottom());
                        this.scroll_handle.scroll_to_item(
                            line_count.saturating_sub(1),
                            gpui_kit::ScrollStrategy::Bottom,
                        );
                        cx.notify();
                    });
                }),
        )
        .child(
            Button::new("logs-follow")
                .label("Follow")
                .xsmall()
                .ghost()
                .tab_stop(false)
                .toggled(following)
                .tooltip("Follow new lines as they arrive")
                .on_click(move |_event, _window, cx| {
                    let _ = this_follow.update(cx, |this: &mut LogsPanel, cx| {
                        this.view.update(cx, |view, _| match view.follow_state() {
                            FollowState::Following => view.scroll_up(),
                            FollowState::Paused => view.scroll_to_bottom(),
                        });
                        cx.notify();
                    });
                }),
        )
}
