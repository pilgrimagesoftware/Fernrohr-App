//! The shell's dock panel: its session (started on open, ended when the panel
//! closes) and the terminal it runs in. What it draws is `render`'s.
//!
//! A session that ends - the shell exits, the container stops or is deleted -
//! leaves the panel open with its screen and scrollback, marked ended. A panel restored
//! from a saved layout starts no session: the one it showed ended when the app
//! quit, and opening a shell is the user's call.

use super::transport::{ExecEnd, ExecTransport};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::*;
use gpui_terminal::{Terminal, TerminalBuilder, TerminalEvent, TerminalView};
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
    /// Over: how, when the cluster said.
    Ended(ExecEnd),
}

/// The terminal a shell runs in.
pub(crate) type ExecTerminal = Entity<TerminalView<ExecTransport>>;

/// A shell in one container.
pub struct ExecPanel {
    pub(super) target: ExecTarget,
    pub(super) scope: PanelScope,
    connection: Option<Entity<ClusterConnection>>,
    pub(crate) state: SessionState,
    /// The session's terminal, once it has started. Dropping it drops the
    /// transport, which ends the session.
    pub(crate) terminal: Option<ExecTerminal>,
    /// Why input just failed to reach the shell - its queue full, say -
    /// shown under the terminal for a moment.
    pub(crate) input_notice: Option<String>,
    /// Clears `input_notice`; replaced by the next one.
    input_notice_timer: Option<Task<()>>,
    pub(super) focus_handle: FocusHandle,
}

impl ExecPanel {
    pub fn new(
        target: ExecTarget,
        scope: PanelScope,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;
        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.start(cx))
            .detach();
        let mut this = Self::build(target, scope, Some(connection), SessionState::Waiting, cx);
        this.start(cx);
        this
    }

    /// A panel whose session is already over - a restored one.
    fn ended(
        target: ExecTarget,
        scope: PanelScope,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let ended = SessionState::Ended(ExecEnd {
            code: None,
            reason: Some(
                "This shell ended when Fernrohr closed. Open a new one from the Pods panel.".into(),
            ),
        });
        Self::build(target, scope, None, ended, cx)
    }

    fn build(
        target: ExecTarget,
        scope: PanelScope,
        connection: Option<Entity<ClusterConnection>>,
        state: SessionState,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            target,
            scope,
            connection,
            state,
            terminal: None,
            input_notice: None,
            input_notice_timer: None,
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
        let (ended_tx, ended) = mpsc::channel(1);
        let runtime = crate::runtime::handle(cx);
        let target = self.target.clone();
        let terminal = TerminalBuilder::new()
            .connect(|sink| {
                Ok::<_, std::convert::Infallible>(ExecTransport::spawn(
                    &runtime, client, target, sink, ended_tx,
                ))
            })
            .unwrap_or_else(|never| match never {});
        self.attach(terminal, ended, cx);
    }

    /// Shows `terminal` and follows the session's end.
    pub(crate) fn attach(
        &mut self,
        terminal: Terminal<ExecTransport>,
        ended: mpsc::Receiver<ExecEnd>,
        cx: &mut Context<Self>,
    ) {
        let style = super::theme::terminal_style(cx);
        let view = cx.new(|cx| TerminalView::new(terminal, style, cx));
        cx.subscribe(&view, |this, _, event: &TerminalEvent, cx| {
            if let TerminalEvent::TransportFailed(error) = event {
                this.show_input_failure(error.to_string(), cx);
            }
        })
        .detach();
        self.terminal = Some(view);
        self.state = SessionState::Running;
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(ended, |end| {
                let _ = this.update(cx, |this, cx| {
                    this.state = SessionState::Ended(end);
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
        cx.notify();
    }

    /// Shows - and logs - why input didn't reach the shell, for a moment.
    fn show_input_failure(&mut self, message: String, cx: &mut Context<Self>) {
        log::warn!(
            "shell input for {}/{}: {message}",
            self.target.pod,
            self.target.container
        );
        self.input_notice = Some(message);
        self.input_notice_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::consts::EXEC_INPUT_NOTICE)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.input_notice = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// A panel over `terminal` - a test's stand-in session - whose end is
    /// `ended`.
    #[cfg(test)]
    pub(crate) fn with_session(
        target: ExecTarget,
        scope: PanelScope,
        terminal: Terminal<ExecTransport>,
        ended: mpsc::Receiver<ExecEnd>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::build(target, scope, None, SessionState::Waiting, cx);
        this.attach(terminal, ended, cx);
        this
    }

    /// What closing the panel would cost, when it costs something: a running
    /// session ends with it (`panel-move-keybindings`' Close Group asks first).
    pub(crate) fn close_warning(&self) -> Option<crate::ui::confirm_text::ConfirmText> {
        (self.state == SessionState::Running).then(|| {
            crate::ui::confirm_text::ConfirmText::from("The shell in ")
                .name(&self.target.pod)
                .text(" \u{00b7} ")
                .name(&self.target.container)
                .text(" ends.")
        })
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
