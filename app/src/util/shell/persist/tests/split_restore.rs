//! fix-split-restore: a nested split's sizes survive a save and restore.

// Named imports, for the reason `super`'s note gives.
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, SavedDockLayouts, WindowMode, save};
use gpui_kit::{AppContext as _, TestAppContext};

/// fix-split-restore: a pane split in two inside a split restores at the sizes
/// it was saved at. Split the centre horizontally, split its left pane
/// vertically, save, then open a second window on the same context, which
/// restores the saved arrangement.
///
/// The dock saves on `LayoutChanged`, which fires before any frame has laid out
/// the new vertical split, so its slots were saved as the 100px placeholder.
/// On restart that came back as a 100px top pane above a bottom pane holding
/// the rest. `save` refreshes the saved arrangement from the dock as drawn.
#[gpui_kit::test]
async fn a_nested_split_round_trips_its_sizes(cx: &mut TestAppContext) {
    use gpui_kit::component::Placement;
    use gpui_kit::component::dock::{DockAreaState, DockPlacement, InsertTarget, PanelState};
    use gpui_kit::{Entity, Pixels};
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        cx.set_global(SavedDockLayouts(
            crate::config::dock_layouts::DockLayouts::default(),
        ));
    });
    // Under `Root`, as `open_window` builds it: `save` writes main windows only.
    let open = |cx: &mut TestAppContext| {
        let mut built = None;
        let handle = cx.add_window(|window, cx| {
            let main = cx.new(|cx| {
                let mut main_window = MainWindow {
                    mode: WindowMode::Picker(
                        cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                    ),
                    focus_handle: cx.focus_handle(),
                };
                main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
                main_window
            });
            built = Some(main.clone());
            gpui_kit::component::Root::new(main, window, cx)
        });
        (handle, built.expect("the window built its view"))
    };
    let live = |main: &Entity<MainWindow>, cx: &mut TestAppContext| -> DockAreaState {
        main.read_with(cx, |main_window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.read(cx).dump(cx)
        })
    };
    let saved = |cx: &mut TestAppContext| -> DockAreaState {
        cx.update(|cx| cx.global::<SavedDockLayouts>().0.get("kind-dev").cloned())
            .expect("the arrangement is saved under its context")
    };
    // The outer split's sizes, then the nested one's.
    fn split_sizes(state: &DockAreaState) -> (Vec<Pixels>, Vec<Pixels>) {
        let sizes = |panel: &PanelState| panel.info.sizes().cloned().expect("a split");
        (sizes(&state.center), sizes(&state.center.children[0]))
    }

    let (first_window, first) = open(cx);
    cx.run_until_parked();
    first_window
        .update(cx, |_, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &first.read(cx).mode else {
                panic!("a connected window is in workspace mode")
            };
            let dock_area = dock_area.clone();
            let kinds = ["configmaps", "secrets"].map(|plural| {
                crate::ui::panel_title::PanelScope::new(
                    crate::ui::nav::NavTarget::Kind(
                        crate::k8s::cluster::discovery::DiscoveredKind {
                            gvk: kube::core::GroupVersionKind::gvk("", "v1", plural),
                            plural: plural.into(),
                            namespaced: true,
                            verbs: Default::default(),
                        },
                    ),
                    "kind-dev".into(),
                )
            });
            dock_area.update(cx, |area, cx| {
                let [right, bottom] =
                    kinds.map(|scope| crate::ui::nav::add_panel(area, &scope, None, window, cx).0);
                // Moves, not `split_at`: that inserts without detaching, so on
                // a panel already docked it leaves the panel in two groups.
                let split = |node, placement| InsertTarget::Split {
                    node,
                    placement,
                    size: None,
                };
                let group = crate::ui::panel::tabs::group_of(area, right).expect("a tab group");
                area.move_panel(right, split(group, Placement::Right), window, cx);
                let left = area
                    .layout(DockPlacement::Center)
                    .and_then(|tree| tree.panels().next())
                    .and_then(|panel| crate::ui::panel::tabs::group_of(area, panel))
                    .expect("the left pane's group");
                area.move_panel(bottom, split(left, Placement::Bottom), window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

    let (_, drawn_nested) = split_sizes(&live(&first, cx));
    let (_, edit_nested) = split_sizes(&saved(cx));
    assert_ne!(
        edit_nested, drawn_nested,
        "the edit's own save predates the nested split's first layout"
    );

    cx.update(|cx| save(cx, &path));
    let at_save = saved(cx);
    assert_eq!(
        split_sizes(&at_save),
        split_sizes(&live(&first, cx)),
        "saving records the sizes as drawn"
    );

    let (_second_window, second) = open(cx);
    cx.run_until_parked();
    let (outer, nested) = split_sizes(&live(&second, cx));
    let (saved_outer, saved_nested) = split_sizes(&at_save);
    let close = |a: &[Pixels], b: &[Pixels]| {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(a, b)| (a.as_f32() - b.as_f32()).abs() < 1.)
    };
    assert!(
        close(&outer, &saved_outer),
        "the horizontal split restores: {outer:?} vs {saved_outer:?}"
    );
    assert!(
        close(&nested, &saved_nested),
        "the vertical pair restores at its saved sizes, not 100px over the rest: \
         {nested:?} vs {saved_nested:?}"
    );

    let _ = std::fs::remove_file(&path);
}
