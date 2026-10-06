// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::*;
use crate::util::shell::{MainWindow, WindowLayout, WindowMode, open_window, watch_picker};
use gpui_kit::component::dock::DockLayout;
use gpui_kit::{App, AppContext as _, Entity, TestAppContext};

/// Section 4.4, amended by `tab-close-buttons` 4.1: emptying a workspace's
/// center dock (what closing its last panel leaves behind) keeps the window in
/// `Workspace` mode, still connected - `watch_workspace`'s
/// `DockEvent::LayoutChanged` subscription, driven directly here via `set_center`
/// with an empty layout rather than a real interactive panel close.
#[gpui_kit::test]
async fn closing_the_last_panel_keeps_the_window_connected(cx: &mut TestAppContext) {
    // See `connected_window`'s doc comment: `enter_workspace` starts a real
    // connect whose completion wakes GPUI from a tokio thread.
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });

    let window = cx.add_window(|window, cx| {
        // Goes through the same transition a real connect does, so this
        // covers the Resource panel being wired up as well as the dock.
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });

    window
        .update(cx, |main_window, _window, _cx| {
            assert!(matches!(main_window.mode, WindowMode::Workspace { .. }));
        })
        .unwrap();

    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                unreachable!("just asserted Workspace mode above");
            };
            dock_area.update(cx, |area, cx| {
                area.set_center(DockLayout::tabs(), window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            assert!(matches!(main_window.mode, WindowMode::Workspace { .. }));
        })
        .unwrap();
}

/// Tasks.md 2.1: `open_window` restores every context a saved layout names, not
/// just the one it builds a dock around - a hold on each (so its session, and
/// the Resource panel's future "already connected" reuse, exist) even though
/// this narrow slice opens no panel at all for a context past the first.
#[gpui_kit::test]
async fn open_window_restores_every_context_even_one_with_no_panels(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });

    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();

    let windows = cx.update(|cx| cx.windows());
    assert_eq!(windows.len(), 1);
    // `open_window` wraps its `MainWindow` in a `gpui_kit::component::Root`
    // (for dialogs/menus), so reaching it back from the window handle goes
    // through the root's own view rather than a direct downcast of the handle.
    let main_window: Entity<MainWindow> = windows[0]
        .update(cx, |_, window, cx| {
            let root = window
                .root::<gpui_kit::component::Root>()
                .flatten()
                .expect("open_window always mounts a Root");
            root.read(cx)
                .view()
                .clone()
                .downcast::<MainWindow>()
                .expect("the Root wraps a MainWindow")
        })
        .unwrap();

    main_window.read_with(cx, |main_window, _cx| {
        let WindowMode::Workspace { contexts, .. } = &main_window.mode else {
            panic!("a restored layout with contexts opens straight into a workspace")
        };
        assert_eq!(
            contexts,
            &vec!["kind-dev".to_string(), "staging".to_string()],
            "both restored contexts are on the window, in order"
        );
    });
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        1
    );
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        1,
        "the second context is held even though it has no panel open yet"
    );
}

/// Regression test for the HANDOFF.md report: opening a second window and
/// selecting a context that a *first* window already connected said "connected"
/// but never switched the second window out of picker mode. Drives both windows
/// through the real `ClusterPicker::select` -> `PickerEvent::Connected` ->
/// `watch_picker` path (unlike `connected_window`, which shortcuts straight to
/// `enter_workspace` and so never exercised this path at all) - the same shared
/// connection entity stands in for `ClusterRegistry` returning the first window's
/// already-`Connected` entity to the second window's picker.
#[gpui_kit::test]
async fn second_window_connecting_to_an_already_connected_context_shows_workspace(
    cx: &mut TestAppContext,
) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::ui::picker::ClusterPicker;
    use kube::{Client, Config};
    use std::cell::RefCell;

    thread_local! {
        static SHARED: RefCell<Option<Entity<ClusterConnection>>> = const { RefCell::new(None) };
    }

    // `connection_factory` only stubs the picker's own connection entity;
    // `watch_picker` still drives `enter_workspace`, which starts a real
    // `ClusterRegistry` connect for the panel it builds. See
    // `connected_window`'s doc comment for why that needs `allow_parking`.
    cx.executor().allow_parking();

    fn shared_connected_stub(cx: &mut App, _context_name: &str) -> Entity<ClusterConnection> {
        SHARED.with(|cell| {
            if let Some(entity) = cell.borrow().as_ref() {
                return entity.clone();
            }
            let handle = crate::runtime::handle(cx);
            let _guard = handle.enter();
            let client =
                Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap();
            let entity =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            *cell.borrow_mut() = Some(entity.clone());
            entity
        })
    }

    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });

    fn picker_window(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<MainWindow> {
        cx.add_window(|window, cx| {
            let picker = cx.new(|cx| {
                let mut picker = ClusterPicker::new(window, cx);
                picker.connection_factory = Some(shared_connected_stub);
                picker
            });
            let main_window = MainWindow {
                mode: WindowMode::Picker(picker.clone()),
                focus_handle: cx.focus_handle(),
            };
            watch_picker(&picker, window, cx);
            main_window
        })
    }

    let first = picker_window(cx);
    first
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Picker(picker) = &main_window.mode else {
                unreachable!("just constructed in Picker mode");
            };
            picker.update(cx, |picker, cx| {
                picker.select("kind-dev".to_string(), cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    first
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "first window connects normally"
            );
        })
        .unwrap();

    let second = picker_window(cx);
    second
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Picker(picker) = &main_window.mode else {
                unreachable!("just constructed in Picker mode");
            };
            picker.update(cx, |picker, cx| {
                picker.select("kind-dev".to_string(), cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    second
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "second window selecting an already-connected context must also \
                 switch to the workspace, not stay stuck showing the picker"
            );
        })
        .unwrap();

    SHARED.with(|cell| *cell.borrow_mut() = None);
}

/// Hypothesis two from `second-window-connect-fix/design.md`: a second window
/// connecting to a context that is *not* already connected (so it goes through
/// `cx.observe`'s callback, not `select`'s synchronous `emit_connected` call).
#[gpui_kit::test]
async fn second_window_connecting_to_a_fresh_context_shows_workspace(cx: &mut TestAppContext) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::ui::picker::ClusterPicker;
    use kube::{Client, Config};

    // See the previous test: `connection_factory` stubs only the picker's
    // own entity, not the real connect `enter_workspace` starts.
    cx.executor().allow_parking();

    fn connecting_then_connected_stub(
        cx: &mut App,
        _context_name: &str,
    ) -> Entity<ClusterConnection> {
        let handle = crate::runtime::handle(cx);
        let _guard = handle.enter();
        let client = Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap();
        let entity = cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
        entity.update(cx, |connection, cx| {
            connection.state = ConnectionState::Connected(client);
            cx.notify();
        });
        entity
    }

    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });

    let second = cx.add_window(|window, cx| {
        let picker = cx.new(|cx| {
            let mut picker = ClusterPicker::new(window, cx);
            picker.connection_factory = Some(connecting_then_connected_stub);
            picker
        });
        let main_window = MainWindow {
            mode: WindowMode::Picker(picker.clone()),
            focus_handle: cx.focus_handle(),
        };
        watch_picker(&picker, window, cx);
        main_window
    });
    second
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Picker(picker) = &main_window.mode else {
                unreachable!("just constructed in Picker mode");
            };
            picker.update(cx, |picker, cx| {
                picker.select("fresh-dev".to_string(), cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    second
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "a fresh connect completing after select must still flip this \
                 window to the workspace"
            );
        })
        .unwrap();
}

/// `cluster-picker-and-navigation`'s amended cluster-picker delta: a saved window
/// with a context but no open panels restores straight into its workspace, not
/// the picker, showing the empty panel area's hint.
#[gpui_kit::test]
async fn a_saved_window_with_contexts_but_no_panels_restores_its_workspace(
    cx: &mut TestAppContext,
) {
    use crate::util::shell::SavedDockLayouts;
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        cx.set_global(SavedDockLayouts(
            crate::config::dock_layouts::DockLayouts::default(),
        ));
    });
    // Save an emptied dock for `kind-dev` the way closing its last panel does.
    let first = cx.add_window(|window, cx| {
        let mut main_window = MainWindow::test_picker_window(window, cx);
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });
    first
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.update(cx, |area, cx| {
                area.set_center(DockLayout::tabs(), window, cx)
            });
        })
        .unwrap();
    cx.run_until_parked();

    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string()],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();

    let restored = cx
        .update(|cx| cx.windows())
        .into_iter()
        .find(|handle| handle.window_id() != first.window_id())
        .expect("the restored window opened");
    let (workspace, panels) = restored
        .update(cx, |_, window, cx| {
            let root = window
                .root::<gpui_kit::component::Root>()
                .flatten()
                .expect("open_window always mounts a Root");
            let main_window = root
                .read(cx)
                .view()
                .clone()
                .downcast::<MainWindow>()
                .expect("the Root wraps a MainWindow");
            match &main_window.read(cx).mode {
                WindowMode::Workspace { open_panels, .. } => (true, open_panels.len()),
                WindowMode::Picker(_) => (false, 0),
            }
        })
        .unwrap();
    assert!(
        workspace,
        "the window restores into its workspace, not the picker"
    );
    assert_eq!(panels, 0, "with no panels open");
    let mut vcx = gpui_kit::VisualTestContext::from_window(restored, cx);
    vcx.run_until_parked();
    assert!(
        vcx.debug_bounds("empty-dock-hint").is_some(),
        "the empty panel area names the key to open a kind"
    );
}
