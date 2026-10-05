//! `command-tunnels` 4.1: the kind switch keeps both forms' values, each kind saves,
//! each command-tunnel validation error is named on its field, and the switch works
//! from the keyboard.

// NAMED imports only (no `use super::*;`): a glob of `gpui_kit::*` next to
// `#[gpui_kit::test]` shadows the builtin `#[test]` and blows the macro-expansion budget.
use crate::config::tunnels::{CommandTunnelMode, TunnelKind, TunnelsConfig};
use crate::tunnel::store::TunnelFieldError;
use crate::ui::tunnels::editor::TunnelEditor;
use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle};
use std::path::{Path, PathBuf};

fn temp_tunnels_path() -> PathBuf {
    crate::util::test_paths::temp_path("tunnel-command-form")
}

fn new_editor(cx: &mut TestAppContext, path: &Path) -> WindowHandle<TunnelEditor> {
    cx.update(gpui_kit::init);
    cx.add_window({
        let path = path.to_path_buf();
        move |window, cx| TunnelEditor::create(path, window, cx)
    })
}

/// Fills the command form and saves, returning the field errors.
fn save_command(
    cx: &mut TestAppContext,
    editor: WindowHandle<TunnelEditor>,
    command: &str,
    port: &str,
    timeout: &str,
) -> Vec<TunnelFieldError> {
    editor
        .update(cx, |editor, window, cx| {
            editor.set_kind(TunnelKind::Command, cx);
            editor
                .name
                .update(cx, |s, cx| s.set_value("QA IAP", window, cx));
            editor
                .command_line
                .update(cx, |s, cx| s.set_value(command.to_string(), window, cx));
            editor
                .local_port
                .update(cx, |s, cx| s.set_value(port.to_string(), window, cx));
            editor
                .startup_timeout
                .update(cx, |s, cx| s.set_value(timeout.to_string(), window, cx));
            editor.save(window, cx);
            editor.field_errors.clone()
        })
        .unwrap()
}

#[gpui_kit::test]
async fn switching_kinds_keeps_both_forms(cx: &mut TestAppContext) {
    let path = temp_tunnels_path();
    let editor = new_editor(cx, &path);
    editor
        .update(cx, |editor, window, cx| {
            editor
                .host
                .update(cx, |s, cx| s.set_value("bastion.example.com", window, cx));
            editor.set_kind(TunnelKind::Command, cx);
            editor.command_line.update(cx, |s, cx| {
                s.set_value("ssh -N -L{port}:127.0.0.1:8888 b", window, cx)
            });
            editor.set_kind(TunnelKind::Ssh, cx);
            assert_eq!(editor.host.read(cx).value(), "bastion.example.com");
            editor.set_kind(TunnelKind::Command, cx);
            assert_eq!(
                editor.command_line.read(cx).value(),
                "ssh -N -L{port}:127.0.0.1:8888 b"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
async fn a_command_tunnel_saves_its_settings(cx: &mut TestAppContext) {
    let path = temp_tunnels_path();
    let editor = new_editor(cx, &path);
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_mode(CommandTunnelMode::Forward, cx)
        })
        .unwrap();
    let errors = save_command(
        cx,
        editor,
        "gcloud compute ssh <host> \\\n  -- -N -L8888:127.0.0.1:8888",
        "8888",
        "45",
    );
    assert!(errors.is_empty(), "{errors:?}");

    let saved: TunnelsConfig = crate::config::load(&path);
    let (_, tunnel) = saved.tunnels.into_iter().next().expect("saved");
    assert_eq!(tunnel.kind, TunnelKind::Command);
    assert_eq!(tunnel.command.mode, CommandTunnelMode::Forward);
    assert_eq!(tunnel.command.local_port, Some(8888));
    assert_eq!(tunnel.command.startup_timeout_secs, 45);
    assert!(
        tunnel.command.command_line.contains("\\\n"),
        "the pasted shape is kept"
    );

    // Back to SSH: saved as SSH, the command settings still there to flip back to.
    editor
        .update(cx, |editor, window, cx| {
            editor.set_kind(TunnelKind::Ssh, cx);
            editor
                .host
                .update(cx, |s, cx| s.set_value("bastion.example.com", window, cx));
            editor
                .user
                .update(cx, |s, cx| s.set_value("ops", window, cx));
            editor.save(window, cx);
            assert!(editor.field_errors.is_empty());
        })
        .unwrap();
    let saved: TunnelsConfig = crate::config::load(&path);
    let (_, tunnel) = saved.tunnels.into_iter().next().expect("saved");
    assert_eq!(tunnel.kind, TunnelKind::Ssh);
    assert_eq!(tunnel.command.local_port, Some(8888));
    let _ = std::fs::remove_file(&path);
}

#[gpui_kit::test]
async fn each_command_error_is_named_on_its_field(cx: &mut TestAppContext) {
    let path = temp_tunnels_path();
    for (command, port, timeout, expected) in [
        ("   ", "", "30", TunnelFieldError::EmptyCommand),
        (
            "ssh 'unclosed {port}",
            "",
            "30",
            TunnelFieldError::UnbalancedQuotes,
        ),
        (
            "ssh -N -L8888:x:1 host",
            "",
            "30",
            TunnelFieldError::NoPortPlaceholder,
        ),
        (
            "ssh -L{port}:x:1 host",
            "70000",
            "30",
            TunnelFieldError::InvalidLocalPort,
        ),
        (
            "ssh -L{port}:x:1 host",
            "",
            "soon",
            TunnelFieldError::InvalidTimeout,
        ),
    ] {
        let editor = new_editor(cx, &path);
        let errors = save_command(cx, editor, command, port, timeout);
        assert_eq!(
            errors,
            [expected],
            "for {command:?} / {port:?} / {timeout:?}"
        );
    }
    let saved: TunnelsConfig = crate::config::load(&path);
    assert!(saved.tunnels.is_empty(), "nothing invalid was written");
}

/// Space down and up: a `Button` clicks on the key's release.
fn press_space(vcx: &mut VisualTestContext) {
    let space = gpui_kit::Keystroke::parse("space").expect("valid");
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    vcx.run_until_parked();
}

/// The kind switch is the pane's first tab stop: Tab reaches SSH, Tab again
/// Command, and Space presses it - in a window with gpui-kit's `Root`, which is what
/// handles Tab, as the Tunnels window has.
#[gpui_kit::test]
async fn the_kind_switch_works_from_the_keyboard(cx: &mut TestAppContext) {
    use gpui_kit::AppContext as _;
    let path = temp_tunnels_path();
    cx.update(gpui_kit::init);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let editor = cx.new(|cx| TunnelEditor::create(path.clone(), window, cx));
        built = Some(editor.clone());
        gpui_kit::component::Root::new(editor, window, cx)
    });
    let editor = built.expect("the window built its editor");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| editor.read(cx).focus_handle.clone().focus(window, cx));
    vcx.run_until_parked();
    let kind = |vcx: &mut VisualTestContext| vcx.update(|_, cx| editor.read(cx).kind);

    vcx.simulate_keystrokes("tab tab");
    press_space(&mut vcx);
    assert_eq!(kind(&mut vcx), TunnelKind::Command);

    vcx.simulate_keystrokes("shift-tab");
    press_space(&mut vcx);
    assert_eq!(kind(&mut vcx), TunnelKind::Ssh);
}
