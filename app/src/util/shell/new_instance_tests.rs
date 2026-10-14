//! `open-panel-new-instance` in a real window: Open in New Panel (`cmd-shift-d`)
//! opens another instance of the focused panel, of any kind (2.1); default
//! opens still focus the first instance, and new instances take the lowest
//! free number (1.2); a duplicated panel's instance is saved and restored, an
//! older layout restores as the first instance (4.1), and layout Add opens a
//! saved second instance beside an open first one (4.2).

use super::panels::{PanelKey, restored_panel_keys};
use super::test_support::{press, workspace};
use super::{MainWindow, WindowMode};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::nav::{NavTarget, ObjectTarget, OpenMode};
use gpui_kit::component::dock::{DockAreaState, PanelId};
use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle};

fn deployments() -> DiscoveredKind {
    DiscoveredKind {
        gvk: kube::core::GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

fn web_deployment() -> NavTarget {
    NavTarget::Object(ObjectTarget {
        kind: deployments(),
        namespace: Some("team-a".into()),
        name: "web".into(),
    })
}

fn open(
    window: WindowHandle<MainWindow>,
    vcx: &mut VisualTestContext,
    target: NavTarget,
    namespaces: Vec<String>,
    mode: OpenMode,
) {
    window
        .update(vcx, |main, window, cx| {
            main.open_target_in(target, None, None, namespaces, mode, window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
}

/// The open panels on `target`: each one's key and dock id, in opening order.
fn instances(
    window: WindowHandle<MainWindow>,
    vcx: &mut VisualTestContext,
    target: &NavTarget,
) -> Vec<(PanelKey, PanelId)> {
    window
        .update(vcx, |main, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.mode else {
                return Vec::new();
            };
            open_panels
                .iter()
                .filter(|open| &open.key.target == target)
                .map(|open| (open.key.clone(), open.id))
                .collect()
        })
        .unwrap()
}

/// Closes the object detail panel `id`, as its close button does, and lets the
/// window forget it. (`util::shell::close_panel` wants a `Root`, which this
/// test window hasn't.)
fn close(window: WindowHandle<MainWindow>, vcx: &mut VisualTestContext, id: PanelId) {
    window
        .update(vcx, |main, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return;
            };
            let dock_area = dock_area.clone();
            if let Some(crate::ui::nav::OpenedPanel::ObjectDetail(panel)) =
                crate::ui::nav::opened_panel_for(dock_area.read(cx), id, cx)
            {
                dock_area.update(cx, |area, cx| area.remove_panel(panel, window, cx));
            }
            main.forget_closed_panels(&dock_area, cx);
        })
        .unwrap();
    vcx.run_until_parked();
}

fn numbers(entries: &[(PanelKey, PanelId)]) -> Vec<u32> {
    entries.iter().map(|(key, _)| key.instance).collect()
}

/// Whether the dock panel `id` holds keyboard focus.
fn has_focus(window: WindowHandle<MainWindow>, vcx: &mut VisualTestContext, id: PanelId) -> bool {
    window
        .update(vcx, |main, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return false;
            };
            dock_area
                .read(cx)
                .panel(id)
                .is_some_and(|view| view.focus_handle(cx).contains_focused(window, cx))
        })
        .unwrap()
}

/// 2.1: `cmd-shift-d` on a focused list, pod detail, object detail and Events
/// browser opens a second instance of each, numbered 1 internally, with focus.
#[gpui_kit::test]
async fn cmd_shift_d_opens_another_instance_of_any_focused_panel(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let cases = [
        (NavTarget::Kind(deployments()), vec!["team-a".to_string()]),
        (NavTarget::pod("team-a", "web-1"), Vec::new()),
        (web_deployment(), Vec::new()),
        (NavTarget::Kind(DiscoveredKind::events()), Vec::new()),
    ];
    for (target, namespaces) in cases {
        open(
            window,
            &mut vcx,
            target.clone(),
            namespaces.clone(),
            OpenMode::Foreground,
        );
        press(&mut vcx, "cmd-shift-d");
        let open = instances(window, &mut vcx, &target);
        assert_eq!(numbers(&open), [0, 1], "{target:?}");
        assert_eq!(open[1].0.namespaces, namespaces, "the same scope");
        assert!(
            has_focus(window, &mut vcx, open[1].1),
            "the new instance has focus"
        );
    }
}

/// 1.2: a default open of a duplicated resource focuses the first instance
/// and opens nothing; new instances number 1, 2, then reuse a freed 1.
#[gpui_kit::test]
async fn default_opens_focus_the_first_and_numbers_are_reused(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let target = web_deployment();
    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::Foreground,
    );
    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::NewPanel,
    );
    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::NewPanel,
    );
    let open_now = instances(window, &mut vcx, &target);
    assert_eq!(numbers(&open_now), [0, 1, 2]);

    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::Foreground,
    );
    assert_eq!(instances(window, &mut vcx, &target).len(), 3, "no fourth");
    assert!(
        has_focus(window, &mut vcx, open_now[0].1),
        "the first is focused"
    );

    // Close the second instance, then open another: it takes the freed number.
    let second = open_now[1].1;
    close(window, &mut vcx, second);
    assert_eq!(numbers(&instances(window, &mut vcx, &target)), [0, 2]);
    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::NewPanel,
    );
    let mut after = numbers(&instances(window, &mut vcx, &target));
    after.sort();
    assert_eq!(after, [0, 1, 2]);
}

/// 2.1: re-scoping one instance of a list leaves the other's scope alone.
#[gpui_kit::test]
async fn instances_keep_their_own_scope(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let target = NavTarget::Kind(deployments());
    open(
        window,
        &mut vcx,
        target.clone(),
        vec!["team-a".into()],
        OpenMode::Foreground,
    );
    press(&mut vcx, "cmd-shift-d");
    let open_now = instances(window, &mut vcx, &target);
    window
        .update(&mut vcx, |main, _window, cx| {
            main.rescope(open_now[1].1, vec!["team-b".into()], cx)
        })
        .unwrap();
    let scopes: Vec<Vec<String>> = instances(window, &mut vcx, &target)
        .into_iter()
        .map(|(key, _)| key.namespaces)
        .collect();
    assert_eq!(
        scopes,
        [vec!["team-a".to_string()], vec!["team-b".to_string()]]
    );
}

/// D6: Open in New Panel on a typed label Logs panel opens a new empty typed
/// panel, not a numbered copy.
#[gpui_kit::test]
async fn a_typed_label_panel_opens_a_new_typed_panel(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let typed = NavTarget::LabelLogs(crate::util::logs::LabelLogs::new_typed());
    open(window, &mut vcx, typed, Vec::new(), OpenMode::Foreground);
    press(&mut vcx, "cmd-shift-d");
    let keys: Vec<PanelKey> = window
        .update(&mut vcx, |main, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.mode else {
                return Vec::new();
            };
            open_panels
                .iter()
                .filter(|open| {
                    matches!(&open.key.target, NavTarget::LabelLogs(source) if source.is_typed())
                })
                .map(|open| open.key.clone())
                .collect()
        })
        .unwrap();
    assert_eq!(keys.len(), 2, "two typed panels");
    assert!(keys.iter().all(|key| key.instance == 0), "neither numbered");
    assert_ne!(keys[0].target, keys[1].target, "each its own typed panel");
}

fn dump(window: WindowHandle<MainWindow>, vcx: &mut VisualTestContext) -> DockAreaState {
    window
        .update(vcx, |main, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                panic!("a workspace window");
            };
            dock_area.read(cx).dump(cx)
        })
        .unwrap()
}

/// The instance numbers `state` saves for panels on `target`.
fn saved_numbers(state: &DockAreaState, target: &NavTarget) -> Vec<u32> {
    restored_panel_keys(&state.center)
        .into_iter()
        .flatten()
        .filter(|key| &key.target == target)
        .map(|key| key.instance)
        .collect()
}

/// 4.1: a duplicated list and pod detail save their instance numbers, and a
/// dock loaded from that state saves them again - each panel restored as the
/// instance it was.
#[gpui_kit::test]
async fn instances_are_saved_and_restored(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let list = NavTarget::Kind(deployments());
    let pod = NavTarget::pod("team-a", "web-1");
    open(
        window,
        &mut vcx,
        list.clone(),
        vec!["team-a".into()],
        OpenMode::Foreground,
    );
    open(
        window,
        &mut vcx,
        list.clone(),
        vec!["team-a".into()],
        OpenMode::NewPanel,
    );
    open(
        window,
        &mut vcx,
        pod.clone(),
        Vec::new(),
        OpenMode::Foreground,
    );
    open(
        window,
        &mut vcx,
        pod.clone(),
        Vec::new(),
        OpenMode::NewPanel,
    );
    let state = dump(window, &mut vcx);
    assert_eq!(saved_numbers(&state, &list), [0, 1]);
    assert_eq!(saved_numbers(&state, &pod), [0, 1]);

    window
        .update(&mut vcx, |main, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return;
            };
            dock_area.update(cx, |area, cx| {
                area.load(state, window, cx).expect("the dump loads");
            });
        })
        .unwrap();
    vcx.run_until_parked();
    let again = dump(window, &mut vcx);
    assert_eq!(saved_numbers(&again, &list), [0, 1]);
    assert_eq!(saved_numbers(&again, &pod), [0, 1]);
}

/// 4.1: saved state with no `instance` - every layout from before - keys as
/// the first instance, and a first instance saves none.
#[gpui_kit::test]
async fn state_without_an_instance_is_the_first(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let pod = NavTarget::pod("team-a", "web-1");
    open(
        window,
        &mut vcx,
        pod.clone(),
        Vec::new(),
        OpenMode::Foreground,
    );
    let state = dump(window, &mut vcx);
    let text = serde_json::to_string(&state).expect("a dock state serializes");
    assert!(
        !text.contains("\"instance\""),
        "a first instance saves no number"
    );
    assert_eq!(saved_numbers(&state, &pod), [0]);
}

/// 4.2: with a Service's first detail instance open, layout Add of a layout
/// holding its instances 1 and 2 opens only the second - not a third panel,
/// nor a second copy of the first.
#[gpui_kit::test]
async fn layout_add_opens_a_saved_second_instance_beside_the_first(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let target = web_deployment();
    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::Foreground,
    );
    open(
        window,
        &mut vcx,
        target.clone(),
        Vec::new(),
        OpenMode::NewPanel,
    );
    let state = dump(window, &mut vcx);
    let second = instances(window, &mut vcx, &target)[1].1;
    close(window, &mut vcx, second);
    assert_eq!(numbers(&instances(window, &mut vcx, &target)), [0]);

    let layout = crate::config::saved_layouts::SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: "two".into(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["kind-dev".into()],
        dock: state,
        resource_panel_width: None,
        window_width: 999.0,
        window_height: 555.0,
    };
    window
        .update(&mut vcx, |main, window, cx| {
            main.load_add(layout, window, cx)
        })
        .unwrap();
    vcx.run_until_parked();
    assert_eq!(numbers(&instances(window, &mut vcx, &target)), [0, 1]);
}
