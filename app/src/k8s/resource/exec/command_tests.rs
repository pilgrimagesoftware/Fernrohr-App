//! A library entry's session (`exec-command-library` D1): the exec attaches the
//! entry's argv - not the shell - with a TTY, the target round-trips through a
//! saved panel, and a restored panel shows the session ended and sends the
//! cluster nothing, though the context is connected.

use super::panel::{ExecPanel, SessionState};
use super::target::{ExecTarget, LibraryCommand};
use super::transport::ExecTransport;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_recorder::Recorder;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use gpui_terminal::TerminalBuilder;
use serde_json::json;

fn library_target() -> ExecTarget {
    ExecTarget {
        namespace: "nats-system".into(),
        pod: "nats-box-0".into(),
        container: "nats-box".into(),
        command: Some(LibraryCommand {
            entry_name: "Stream info".into(),
            argv: vec![
                "nats".into(),
                "stream".into(),
                "info".into(),
                "my stream".into(),
            ],
        }),
    }
}

#[test]
fn a_library_target_runs_its_argv_and_a_plain_one_the_shell() {
    assert_eq!(
        library_target().argv(),
        ["nats", "stream", "info", "my stream"]
    );
    let shell = ExecTarget {
        command: None,
        ..library_target()
    };
    assert_eq!(shell.argv()[0], "/bin/sh");
    assert_eq!(
        library_target().title(),
        "Stream info: nats-box-0 · nats-box"
    );
    assert_eq!(shell.title(), "Shell: nats-box-0 · nats-box");
}

#[test]
fn the_command_survives_a_saved_panel() {
    let target = library_target();
    assert_eq!(ExecTarget::from_saved(&target.saved()), Some(target));
    let plain = json!({ "namespace": "shop", "pod": "web-1", "container": "app" });
    assert_eq!(
        ExecTarget::from_saved(&plain).and_then(|target| target.command),
        None,
        "a shell saved before the library has no command"
    );
}

/// The exec request carries the entry's argv as its `command`s, with stdin and
/// a TTY, as the shell's does.
#[gpui_kit::test]
async fn the_exec_attaches_the_entrys_argv(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let runtime = cx.update(|cx| crate::runtime::handle(cx));
    let (server, client) = Recorder::start(&runtime, "400 Bad Request", json!({}));
    let (ended, _ended_rx) = tokio::sync::mpsc::channel(1);
    let _terminal = TerminalBuilder::new()
        .connect(|sink| {
            Ok::<_, std::convert::Infallible>(ExecTransport::spawn(
                &runtime,
                client,
                library_target(),
                sink,
                ended,
            ))
        })
        .unwrap_or_else(|never| match never {});
    for _ in 0..400 {
        if !server.requests().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let target = server.requests()[0].target.clone();
    assert!(
        target.starts_with("/api/v1/namespaces/nats-system/pods/nats-box-0/exec?"),
        "{target}"
    );
    assert!(
        target.contains("command=nats&command=stream&command=info&command=my+stream"),
        "the entry's argv, in order: {target}"
    );
    assert!(
        !target.contains("command=%2Fbin%2Fsh"),
        "not the shell: {target}"
    );
    for param in ["container=nats-box", "stdin=true", "tty=true"] {
        assert!(target.contains(param), "{param}: {target}");
    }
}

/// Spec "Restored panel": restored, the session is ended and nothing is sent,
/// though the context is connected.
#[gpui_kit::test]
async fn a_restored_command_panel_is_ended_and_sends_nothing(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let runtime = cx.update(|cx| crate::runtime::handle(cx));
    let (server, client) = Recorder::start(&runtime, "400 Bad Request", json!({}));
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
    });
    let target = library_target();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Exec(target.clone()), "demo".into());
        let panel = cx.new(|cx| ExecPanel::ended(target, scope, window, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("built");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(50));
    vcx.run_until_parked();

    let state = vcx.update(|_, cx| panel.read(cx).state.clone());
    let SessionState::Ended(end) = state else {
        panic!("restored ended, not {state:?}");
    };
    assert!(
        end.reason.unwrap_or_default().contains("Exec Library"),
        "says how to run it again"
    );
    assert!(
        server.requests().is_empty(),
        "no command sent to the cluster"
    );
}
