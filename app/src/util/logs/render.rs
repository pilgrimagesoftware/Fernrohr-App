//! Drawing the panel: the log line list (or the terminal-state message), the
//! container picker, and the control bar (jump to top/bottom, follow toggle).

use super::*;
use crate::ui::typography::TypeRole as _;

impl Render for LogsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view.read(cx);
        let following = view.follow_state() == FollowState::Following;
        let content = if let Some(message) = view.terminal_message() {
            let message = message.to_string();
            let detail = view.terminal_detail().map(str::to_string);
            panel_title::error_content(message, detail, cx).into_any_element()
        } else if self.current.is_none() {
            div()
                .size_full()
                .p_3()
                .child("Click a pod to view its logs.")
                .into_any_element()
        } else {
            let lines = Rc::new(view.lines().to_vec());
            let line_count = lines.len();
            div()
                .size_full()
                .p_3()
                .code_font(cx)
                .overflow_x_scrollbar()
                .child(
                    uniform_list(
                        "logs-panel-lines",
                        line_count,
                        move |range, _window, _cx| {
                            range
                                .map(|ix| {
                                    div()
                                        .whitespace_nowrap()
                                        .child(lines[ix].clone())
                                        .into_any_element()
                                })
                                .collect()
                        },
                    )
                    .track_scroll(&self.scroll_handle)
                    .size_full(),
                )
                .into_any_element()
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
        let control_bar =
            (self.current.is_some() && view.terminal_message().is_none()).then(|| {
                let line_count = view.lines().len();
                let this_top = cx.weak_entity();
                let this_bottom = cx.weak_entity();
                let this_follow = cx.weak_entity();
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .p_2()
                    .bg(crate::ui::style::surface_raised(cx))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    // "pod / container" (plus the context in a multi-context window) on
                    // the left; the controls take the rest of the row on the right.
                    .child(div().flex_1().min_w_0().children(heading))
                    .children(container_picker)
                    .child(
                        Button::new("logs-jump-top")
                            .icon(gpui_kit::assets::IconName::ArrowUp)
                            .xsmall()
                            .ghost()
                            .tab_stop(false)
                            .tooltip("Jump to top")
                            .on_click(move |_event, _window, cx| {
                                let _ = this_top.update(cx, |this: &mut Self, cx| {
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
                                let _ = this_bottom.update(cx, |this: &mut Self, cx| {
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
                                let _ = this_follow.update(cx, |this: &mut Self, cx| {
                                    this.view.update(cx, |view, _| match view.follow_state() {
                                        FollowState::Following => view.scroll_up(),
                                        FollowState::Paused => view.scroll_to_bottom(),
                                    });
                                    cx.notify();
                                });
                            }),
                    )
            });

        div()
            .size_full()
            // Tracked so a click focuses the panel, which is what lights its
            // tab's focus underline.
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .children(control_bar)
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }
}
