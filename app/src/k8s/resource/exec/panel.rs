//! The shell's dock panel: its session (started on open, ended when the panel
//! closes), its transcript, and its input line. What it draws is `render`'s.
//!
//! A session that ends - the shell exits, the container stops or is deleted -
//! leaves the panel open with its transcript, marked ended. A panel restored
//! from a saved layout starts no session: the one it showed ended when the app
//! quit, and opening a shell is the user's call.

use super::bridge::{self, ExecEvent};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use tokio::sync::mpsc;

/// Which container a shell is in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecTarget {
    pub namespace: String,
    pub pod: String,
    pub container: String,
}

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Exec", |context, window, cx| {
        use crate::ui::unrestored::{required_str, restore_with};
        restore_with(&context, cx, |state, cx| {
            let target = ExecTarget {
                namespace: required_str(state, "namespace")?.to_string(),
                pod: required_str(state, "pod")?.to_string(),
                container: required_str(state, "container")?.to_string(),
            };
            let context_name = required_str(state, "context_name")?.to_string();
            let scope = PanelScope::new(NavTarget::Exec(target.clone()), context_name);
            Ok(panel_handle(
                cx.new(|cx| ExecPanel::ended(target, scope, window, cx)),
            ))
        })
    });
}

/// Whether the session is still going.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SessionState {
    /// Waiting for the cluster connection before it can start.
    Waiting,
    Running,
    /// Over, with the reason when there is one.
    Ended(Option<String>),
}

/// A shell in one container.
pub struct ExecPanel {
    pub(super) target: ExecTarget,
    pub(super) scope: PanelScope,
    connection: Option<Entity<ClusterConnection>>,
    pub(crate) state: SessionState,
    /// Everything the session showed, input lines included, capped at
    /// `consts::EXEC_TRANSCRIPT_LIMIT` bytes from the end.
    pub(crate) transcript: String,
    pub(super) input: Entity<InputState>,
    /// The user's lines, on their way to the shell's stdin.
    stdin: Option<mpsc::Sender<Vec<u8>>>,
    /// The session's task, aborted when the panel goes.
    session: Option<tokio::task::JoinHandle<()>>,
    pub(super) scroll: ScrollHandle,
    pub(super) focus_handle: FocusHandle,
}

impl ExecPanel {
    pub fn new(
        target: ExecTarget,
        scope: PanelScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;
        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.start(cx))
            .detach();
        let mut this = Self::build(
            target,
            scope,
            Some(connection),
            SessionState::Waiting,
            window,
            cx,
        );
        this.start(cx);
        this
    }

    /// A panel whose session is already over - a restored one.
    fn ended(
        target: ExecTarget,
        scope: PanelScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let ended = SessionState::Ended(Some(
            "This shell ended when Fernrohr closed. Open a new one from the Pods panel.".into(),
        ));
        Self::build(target, scope, None, ended, window, cx)
    }

    fn build(
        target: ExecTarget,
        scope: PanelScope,
        connection: Option<Entity<ClusterConnection>>,
        state: SessionState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Command, then Enter"));
        cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let line = input.read(cx).value().to_string();
                    this.send_line(&line, cx);
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                }
            },
        )
        .detach();
        cx.on_release(|this: &mut Self, _| {
            if let Some(session) = this.session.take() {
                session.abort();
            }
        })
        .detach();
        Self {
            target,
            scope,
            connection,
            state,
            transcript: String::new(),
            input,
            stdin: None,
            session: None,
            scroll: ScrollHandle::new(),
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        }
    }

    /// Starts the session once the cluster is connected. A no-op once started.
    fn start(&mut self, cx: &mut Context<Self>) {
        if self.state != SessionState::Waiting {
            return;
        }
        let Some(connection) = &self.connection else {
            return;
        };
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let (input_tx, input_rx) = mpsc::channel(crate::consts::EXEC_INPUT_QUEUE);
        let (events_tx, events_rx) = mpsc::channel(crate::consts::EXEC_OUTPUT_QUEUE);
        let ExecTarget {
            namespace,
            pod,
            container,
        } = self.target.clone();
        self.session = Some(crate::runtime::handle(cx).spawn(bridge::run(
            client, namespace, pod, container, input_rx, events_tx,
        )));
        self.stdin = Some(input_tx);
        self.state = SessionState::Running;
        self.follow(events_rx, cx);
        cx.notify();
    }

    /// Applies the session's events to the panel as they arrive.
    pub(crate) fn follow(&mut self, events: mpsc::Receiver<ExecEvent>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(events, |event| {
                let _ = this.update(cx, |this, cx| this.apply(event, cx));
            })
            .await;
        })
        .detach();
    }

    fn apply(&mut self, event: ExecEvent, cx: &mut Context<Self>) {
        match event {
            ExecEvent::Output(text) => self.append(&text),
            ExecEvent::Ended(reason) => {
                self.state = SessionState::Ended(reason);
                self.stdin = None;
            }
        }
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Sends `line` to the shell and echoes it into the transcript - the shell
    /// has no TTY to echo it.
    fn send_line(&mut self, line: &str, cx: &mut Context<Self>) {
        if self.state != SessionState::Running {
            return;
        }
        let Some(stdin) = &self.stdin else {
            return;
        };
        if stdin.try_send(format!("{line}\n").into_bytes()).is_ok() {
            self.append(&format!("$ {line}\n"));
            self.scroll.scroll_to_bottom();
            cx.notify();
        }
    }

    fn append(&mut self, text: &str) {
        self.transcript.push_str(text);
        let limit = crate::consts::EXEC_TRANSCRIPT_LIMIT;
        if self.transcript.len() > limit {
            let mut cut = self.transcript.len() - limit;
            while !self.transcript.is_char_boundary(cut) {
                cut += 1;
            }
            self.transcript.drain(..cut);
        }
    }

    /// A panel whose session is `events` - a test's stand-in for an exec - and
    /// whose input goes to `stdin`.
    #[cfg(test)]
    pub(crate) fn with_session(
        target: ExecTarget,
        scope: PanelScope,
        stdin: mpsc::Sender<Vec<u8>>,
        events: mpsc::Receiver<ExecEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::build(target, scope, None, SessionState::Running, window, cx);
        this.stdin = Some(stdin);
        this.follow(events, cx);
        this
    }

    /// The input line, for tests to type into.
    #[cfg(test)]
    pub(crate) fn input(&self) -> Entity<InputState> {
        self.input.clone()
    }
}

impl Focusable for ExecPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for ExecPanel {}

impl BasePanel for ExecPanel {
    fn panel_name(&self) -> &'static str {
        "Exec"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "namespace": self.target.namespace,
                "pod": self.target.pod,
                "container": self.target.container,
            })),
        }
    }
}

impl Panel for ExecPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            window,
            cx,
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}
