// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::util::shell::test_support::*;
use crate::util::shell::{MainWindow, NavTarget, OpenedPanel, WindowMode, init};
use gpui_kit::{AppContext as _, TestAppContext};

/// The panel's own keys have to be *bound*, not merely printed.
///
/// The hint bar under the pods table reads the keymap for each shortcut
/// and falls back to printing the letter, so an unbound `d` looks
/// identical to a working one on screen while doing nothing when pressed.
/// This presses the key rather than dispatching the action, because the
/// binding is exactly the part that can be missing.
///
/// At the end of the module on purpose: `title_bar_of` above is being
/// changed on another branch, and a test whose context sits under it would
/// stop applying the moment that lands.
#[gpui_kit::test]
async fn a_pods_panel_shortcut_key_reaches_the_window(cx: &mut TestAppContext) {
    use crate::k8s::resource::pods::{PodSelection, SelectedPod};
    use gpui_kit::{Focusable as _, test::TestWindowExt as _};

    let workspace = temp_workspace_path();
    let keymap = temp_workspace_path();
    // See `connected_window`'s doc comment: `enter_workspace` starts a real
    // connect whose completion wakes GPUI from a tokio thread.
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace.clone(), &keymap);
    });
    let window = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });

    // Focus the pods list, the way clicking into its table would.
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let Some(OpenedPanel::Pods(panel)) = open_panels[0].panel.clone() else {
                panic!("a new workspace opens on the pods list")
            };
            panel.read(cx).focus_handle(cx).focus(window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    // A real keystroke, not a dispatched action. Dispatched through the
    // window rather than the entity: a keypress re-renders, and re-entering
    // the window's view while it is mid-update is what gpui forbids.
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_keystroke(
            gpui_kit::Keystroke::parse("d").expect("valid keystroke"),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("the window is still open");
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.target == NavTarget::pod("default", "web-1")),
                "pressing `d` in the pods list opened the selected pod's detail \
                 panel, so the key was bound rather than only printed"
            );
        })
        .unwrap();

    let _ = std::fs::remove_file(&workspace);
    let _ = std::fs::remove_file(&keymap);
}
