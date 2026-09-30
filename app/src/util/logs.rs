use std::future::Future;

/// One event off a pod container's log stream. Kept separate from the
/// resource-index coalescing path (runtime::drain_coalescing) - logs are
/// ordered text, not keyed state, so every line must render, in order, with
/// nothing folded away.
pub enum LogEvent {
    Line(String),
    /// The stream ended normally (e.g. the pod was deleted).
    Ended,
    /// The stream never started (e.g. the container hasn't started yet).
    RequestFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowState {
    Following,
    /// The user scrolled up to read history; new lines still arrive but the
    /// view doesn't jump to them until they scroll back to the bottom.
    Paused,
}

/// View-model for one pod's log panel: line history, follow state, and
/// container selection. GPUI-free so it's directly unit-testable.
pub struct LogsView {
    lines: Vec<String>,
    follow: FollowState,
    containers: Vec<String>,
    #[allow(dead_code)]
    selected_container: String,
    terminal_message: Option<String>,
}

impl LogsView {
    /// `containers` must be non-empty; the first one is selected by default.
    pub fn new(containers: Vec<String>) -> Self {
        let selected_container = containers.first().cloned().unwrap_or_default();
        Self {
            lines: Vec::new(),
            follow: FollowState::Following,
            containers,
            selected_container,
            terminal_message: None,
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn follow_state(&self) -> FollowState {
        self.follow
    }

    pub fn selected_container(&self) -> &str {
        &self.selected_container
    }

    pub fn containers(&self) -> &[String] {
        &self.containers
    }

    pub fn terminal_message(&self) -> Option<&str> {
        self.terminal_message.as_deref()
    }

    /// Whether a container picker needs to be shown at all - a single
    /// container needs no pick.
    pub fn needs_container_picker(&self) -> bool {
        self.containers.len() > 1
    }

    pub fn append_line(&mut self, line: String) {
        self.lines.push(line);
    }

    pub fn scroll_up(&mut self) {
        self.follow = FollowState::Paused;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.follow = FollowState::Following;
    }

    /// Selects `container`. Returns whether the selection actually changed -
    /// the caller restarts the stream only then, and this clears the history
    /// and terminal state from the previous container's stream. Unused now
    /// that `LogsPanel::switch_container` rebuilds a fresh `LogsView` instead
    /// (matching `sync`'s own pattern) - kept for the unit tests exercising
    /// the view model directly, and as the natural API if that changes.
    #[allow(dead_code)]
    pub fn select_container(&mut self, container: &str) -> bool {
        if container == self.selected_container {
            return false;
        }
        self.selected_container = container.to_string();
        self.lines.clear();
        self.terminal_message = None;
        true
    }

    pub fn apply(&mut self, event: LogEvent) {
        match event {
            LogEvent::Line(line) => self.append_line(line),
            LogEvent::Ended => self.terminal_message = Some("Log stream ended.".into()),
            LogEvent::RequestFailed(reason) => {
                self.terminal_message = Some(format!("Couldn't start log stream: {reason}"));
            }
        }
    }
}

/// Spawns `produce` on the tokio runtime and applies its [`LogEvent`]s to
/// `view` as they arrive, on `view`'s own line-by-line channel - never
/// through the coalescing drain, so every line lands.
///
/// Returns the foreground [`gpui_kit::Task`] driving the drain; drop it (or
/// let a caller-held handle drop) to stop applying further lines, e.g. when
/// switching containers or closing the panel. Production callers that want
/// fire-and-forget behavior can `.detach()` the result themselves.
pub fn start_stream<F, Fut>(
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
    capacity: usize,
    produce: F,
) -> gpui_kit::Task<()>
where
    F: FnOnce(tokio::sync::mpsc::Sender<LogEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let rx = crate::runtime::spawn_stream(cx, capacity, produce);
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |event| {
            view.update(cx, |view, cx| {
                view.apply(event);
                cx.notify();
            });
        })
        .await;
    })
}

/// Streams `container`'s logs in `namespace`/`pod_name` on `client`, line by
/// line, until the stream ends or the returned `Task` is dropped. A failure
/// to start the stream (e.g. the container hasn't started yet) reports
/// through the same channel as [`LogEvent::RequestFailed`] rather than
/// erroring the caller, matching [`LogsView`]'s own terminal-state handling.
pub fn stream_container_logs(
    client: kube::Client,
    namespace: String,
    pod_name: String,
    container: String,
    view: gpui_kit::Entity<LogsView>,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    start_stream(view, cx, 64, move |tx| async move {
        use futures_util::{AsyncBufReadExt, StreamExt};
        use k8s_openapi::api::core::v1::Pod;
        use kube::Api;
        use kube::api::LogParams;

        let api: Api<Pod> = Api::namespaced(client, &namespace);
        let lp = LogParams {
            container: Some(container),
            follow: true,
            ..Default::default()
        };
        let stream = match api.log_stream(&pod_name, &lp).await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = tx.send(LogEvent::RequestFailed(error.to_string())).await;
                return;
            }
        };
        let mut lines = stream.lines();
        loop {
            match lines.next().await {
                Some(Ok(line)) => {
                    if tx.send(LogEvent::Line(line)).await.is_err() {
                        break;
                    }
                }
                None => {
                    let _ = tx.send(LogEvent::Ended).await;
                    break;
                }
                Some(Err(error)) => {
                    let _ = tx.send(LogEvent::RequestFailed(error.to_string())).await;
                    break;
                }
            }
        }
    })
}

use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use std::rc::Rc;

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Logs", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("Logs layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("Logs layout state must name its cluster")
            .to_string();
        let namespaces = serde_json::from_value(state["namespaces"].clone()).unwrap_or_default();
        let scope = PanelScope::new(NavTarget::Logs, context_name).scoped_to(namespaces);
        panel_handle(cx.new(|cx| LogsPanel::new(scope, cx)))
    });
}

/// A dock panel streaming the container logs of whichever pod was last
/// clicked in a Pods panel (see [`SelectedPod`]).
pub struct LogsPanel {
    scope: PanelScope,
    connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
    view: Entity<LogsView>,
    stream: Option<Task<()>>,
    current: Option<(String, String, String)>,
    scroll_handle: UniformListScrollHandle,
    focus_handle: FocusHandle,
}

impl LogsPanel {
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        cx.observe_global::<SelectedPod>(|this: &mut Self, cx| this.sync(cx))
            .detach();

        let mut this = Self {
            scope,
            connection,
            view: cx.new(|_| LogsView::new(vec![String::new()])),
            stream: None,
            current: None,
            scroll_handle: UniformListScrollHandle::default(),
            focus_handle: cx.focus_handle(),
        };
        this.sync(cx);
        this
    }

    /// Starts (or restarts) the stream if the selected pod/container changed
    /// since the last sync. A no-op while nothing is selected or the cluster
    /// isn't connected yet - [`Self::new`]'s observers call this again once
    /// either changes.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = cx.try_global::<SelectedPod>().and_then(|s| s.0.clone()) else {
            return;
        };
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let PodSelection {
            namespace,
            name,
            containers,
        } = selection;
        let container = containers.first().cloned().unwrap_or_default();
        let key = (namespace.clone(), name.clone(), container.clone());
        if self.current.as_ref() == Some(&key) {
            return;
        }
        self.current = Some(key);

        let client = client.clone();
        let view = cx.new(|_| LogsView::new(containers));
        cx.observe(&view, |this: &mut Self, view, cx| {
            if view.read(cx).follow_state() == FollowState::Following {
                let last = view.read(cx).lines().len().saturating_sub(1);
                this.scroll_handle
                    .scroll_to_item(last, gpui_kit::ScrollStrategy::Bottom);
            }
            cx.notify();
        })
        .detach();
        self.stream = Some(stream_container_logs(
            client,
            namespace,
            name,
            container,
            view.clone(),
            cx,
        ));
        self.view = view;
    }

    /// Restarts the stream on `container`, reusing the current pod/namespace -
    /// the container picker's only job, since `LogsView::select_container`
    /// already reports whether the selection actually changed.
    fn switch_container(&mut self, container: String, cx: &mut Context<Self>) {
        let Some((namespace, name, current_container)) = self.current.clone() else {
            return;
        };
        if container == current_container {
            return;
        }
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let containers = self.view.read(cx).containers().to_vec();
        self.current = Some((namespace.clone(), name.clone(), container.clone()));

        let client = client.clone();
        let view = cx.new(|_| LogsView::new(containers));
        cx.observe(&view, |this: &mut Self, view, cx| {
            if view.read(cx).follow_state() == FollowState::Following {
                let last = view.read(cx).lines().len().saturating_sub(1);
                this.scroll_handle
                    .scroll_to_item(last, gpui_kit::ScrollStrategy::Bottom);
            }
            cx.notify();
        })
        .detach();
        self.stream = Some(stream_container_logs(
            client,
            namespace,
            name,
            container,
            view.clone(),
            cx,
        ));
        self.view = view;
    }
}

impl Focusable for LogsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for LogsPanel {}
impl EventEmitter<ScopeEvent> for LogsPanel {}

impl Render for LogsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view.read(cx);
        let following = view.follow_state() == FollowState::Following;
        let content = if let Some(message) = view.terminal_message() {
            div()
                .size_full()
                .p_3()
                .child(message.to_string())
                .into_any_element()
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
                .font_family(cx.theme().mono_font_family.clone())
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

        let control_bar =
            (self.current.is_some() && view.terminal_message().is_none()).then(|| {
                let line_count = view.lines().len();
                let this_top = cx.weak_entity();
                let this_bottom = cx.weak_entity();
                let this_follow = cx.weak_entity();
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
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

        panel_title::focus_frame(
            div()
                .size_full()
                .flex()
                .flex_col()
                .children(control_bar)
                .child(div().flex_1().min_h_0().child(content)),
            &self.focus_handle,
            window,
            cx,
        )
        .into_any_element()
    }
}

impl BasePanel for LogsPanel {
    fn panel_name(&self) -> &'static str {
        "Logs"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "namespaces": self.scope.namespaces,
            })),
        }
    }
}

/// Section 10: the title bar, supplied to the dock rather than drawn here.
/// What a Logs panel's title/tab reads for `current` (namespace, pod,
/// container), once a pod has been selected. `fallback` is whatever the
/// panel shows before that - the generic scope-derived label.
fn streaming_title(
    current: Option<&(String, String, String)>,
    fallback: impl Fn() -> String,
) -> String {
    match current {
        Some((_, pod, container)) => format!("Logs: {pod} · {container}"),
        None => fallback(),
    }
}

impl LogsPanel {
    fn streaming_title(&self) -> String {
        streaming_title(self.current.as_ref(), || panel_title::title(&self.scope))
    }
}

impl Panel for LogsPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(&self.scope, self.streaming_title())
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        panel_title::toolbar_buttons()
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests {
    use super::{FollowState, LogEvent, LogsView, start_stream, streaming_title};
    use gpui_kit::{AppContext as _, TestAppContext};

    #[test]
    fn streaming_title_names_the_pod_and_container() {
        let current = (
            "default".to_string(),
            "web-1".to_string(),
            "app".to_string(),
        );
        assert_eq!(
            streaming_title(Some(&current), || "unreachable".to_string()),
            "Logs: web-1 · app"
        );
    }

    #[test]
    fn streaming_title_falls_back_before_a_pod_is_selected() {
        assert_eq!(streaming_title(None, || "Logs".to_string()), "Logs");
    }

    #[test]
    fn history_renders_and_new_lines_append() {
        let mut view = LogsView::new(vec!["app".into()]);
        view.apply(LogEvent::Line("first".into()));
        view.apply(LogEvent::Line("second".into()));

        assert_eq!(view.lines(), &["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn scrolling_up_pauses_and_returning_to_bottom_resumes() {
        let mut view = LogsView::new(vec!["app".into()]);
        assert_eq!(view.follow_state(), FollowState::Following);

        view.scroll_up();
        assert_eq!(view.follow_state(), FollowState::Paused);

        view.scroll_to_bottom();
        assert_eq!(view.follow_state(), FollowState::Following);
    }

    #[test]
    fn single_container_needs_no_pick() {
        let view = LogsView::new(vec!["app".into()]);
        assert_eq!(view.selected_container(), "app");
        assert!(!view.needs_container_picker());
    }

    #[test]
    fn multi_container_fixture_switches_streams_on_selection() {
        let mut view = LogsView::new(vec!["app".into(), "sidecar".into()]);
        assert_eq!(view.selected_container(), "app");
        assert!(view.needs_container_picker());

        view.apply(LogEvent::Line("from app".into()));
        let changed = view.select_container("sidecar");

        assert!(
            changed,
            "selecting a different container should report a change"
        );
        assert_eq!(view.selected_container(), "sidecar");
        assert!(
            view.lines().is_empty(),
            "switching containers should clear the previous container's history"
        );

        let unchanged = view.select_container("sidecar");
        assert!(!unchanged, "reselecting the current container is a no-op");
    }

    #[test]
    fn deleted_pod_and_not_started_container_show_distinct_terminal_messages() {
        let mut stream_ended = LogsView::new(vec!["app".into()]);
        stream_ended.apply(LogEvent::Line("some log line".into()));
        stream_ended.apply(LogEvent::Ended);

        let mut request_failed = LogsView::new(vec!["app".into()]);
        request_failed.apply(LogEvent::RequestFailed("container not started".into()));

        let ended_message = stream_ended.terminal_message().unwrap();
        let failed_message = request_failed.terminal_message().unwrap();
        assert_ne!(ended_message, failed_message);
        assert!(ended_message.to_lowercase().contains("ended"));
        assert!(
            failed_message
                .to_lowercase()
                .contains("container not started")
        );
    }

    #[gpui_kit::test]
    async fn mock_stream_populates_history_line_by_line(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let view = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            cx.new(|_| LogsView::new(vec!["app".into()]))
        });

        let task = cx.update(|cx| {
            start_stream(view.clone(), cx, 8, |tx| async move {
                for line in ["line one", "line two", "line three"] {
                    tx.send(LogEvent::Line(line.into())).await.unwrap();
                }
            })
        });
        task.await;

        let lines = view.read_with(cx, |view, _| view.lines().to_vec());
        assert_eq!(lines, vec!["line one", "line two", "line three"]);
    }
}
