//! `selector-logs-multiple-panels`: two typed label panels in one context
//! stream independently, each titled by its selector (2.2); a layout holding
//! both restores each separately (3.1); and a layout saved before typed panels
//! had identities still restores its one (3.2).

use super::{is_typed, pod, typed_panels, wait_for_streams, window};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::util::logs::{LabelLogs, LogsPanel, TypedId};
use crate::util::shell::{MainWindow, WindowMode};
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

/// A window over a fake cluster holding `web-1` (`app=web`) and `db-1`
/// (`app=db`), with two typed panels opened from the menu's command, one
/// following each.
fn two_panels(
    cx: &mut TestAppContext,
) -> (
    FakeCluster,
    Entity<MainWindow>,
    VisualTestContext,
    Entity<LogsPanel>,
    Entity<LogsPanel>,
) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", pod("web-1", "web", 0));
    cluster.apply("/api/v1", "pods", pod("db-1", "db", 0));
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    let open = |selector: &str, vcx: &mut VisualTestContext| {
        vcx.update(|window, cx| {
            let focus = main.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
            window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx)
        });
        vcx.run_until_parked();
        let keys: Vec<String> = selector.chars().map(String::from).collect();
        vcx.simulate_keystrokes(&format!("{} enter", keys.join(" ")));
        vcx.run_until_parked();
    };
    open("app=web", &mut vcx);
    open("app=db", &mut vcx);
    let panels = typed_panels(&main, &mut vcx);
    assert_eq!(panels.len(), 2, "two typed panels in one context");
    let (web, db) = (panels[0].clone(), panels[1].clone());
    (cluster, main, vcx, web, db)
}

/// 2.2: each panel streams its own selector's pods alone, and is titled by it.
#[gpui_kit::test]
async fn two_panels_in_one_context_stream_independently(cx: &mut TestAppContext) {
    let (_cluster, _main, mut vcx, web, db) = two_panels(cx);
    wait_for_streams(&mut vcx, &web, &["web-1/app#0@uid-web-1"]);
    wait_for_streams(&mut vcx, &db, &["db-1/app#0@uid-db-1"]);
    let titles = vcx.update(|_, cx| (web.read(cx).test_title(), db.read(cx).test_title()));
    assert_eq!(
        titles,
        ("Logs: app=web".to_string(), "Logs: app=db".to_string())
    );
}

/// The dock's saved state.
fn dump(main: &Entity<MainWindow>, vcx: &mut VisualTestContext) -> DockAreaState {
    vcx.update(|_, cx| {
        let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
            panic!("a workspace window");
        };
        dock_area.read(cx).dump(cx)
    })
}

/// Loads `state` into the window's dock, and returns the Logs panels it built.
fn load(
    main: &Entity<MainWindow>,
    state: DockAreaState,
    vcx: &mut VisualTestContext,
) -> Vec<Entity<LogsPanel>> {
    let mut restored = Vec::new();
    vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return;
            };
            dock_area.update(cx, |area, cx| {
                area.load(state, window, cx).expect("the dump loads");
            });
            restored = main
                .test_dock_views(cx)
                .iter()
                .filter_map(|view| view.view().downcast::<LogsPanel>().ok())
                .collect();
        })
    });
    vcx.run_until_parked();
    restored
}

/// 3.1: a layout holding both panels restores each, with its own selector,
/// under keys of its own.
#[gpui_kit::test]
async fn a_layout_with_two_typed_panels_restores_both(cx: &mut TestAppContext) {
    let (_cluster, main, mut vcx, _web, _db) = two_panels(cx);
    let state = dump(&main, &mut vcx);
    let keys: Vec<_> = super::super::panels::restored_panel_keys(&state.center)
        .into_iter()
        .flatten()
        .filter(|key| is_typed(&key.target))
        .collect();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1], "each panel is keyed apart");

    let restored = load(&main, state, &mut vcx);
    let mut selectors: Vec<Option<String>> = vcx.update(|_, cx| {
        restored
            .iter()
            .map(|panel| panel.read(cx).test_selector())
            .collect()
    });
    selectors.sort();
    assert_eq!(
        selectors,
        [Some("app=db".to_string()), Some("app=web".to_string())]
    );
}

/// `state` with every saved `label_logs` rewritten to the bare `"Typed"` a
/// layout saved before typed panels had identities holds.
fn as_legacy(state: DockAreaState) -> DockAreaState {
    fn rewrite(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(source) = map.get_mut("label_logs")
                    && source.get("Typed").is_some()
                {
                    *source = serde_json::json!("Typed");
                }
                map.values_mut().for_each(rewrite);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(rewrite),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(&state).expect("a dock state serializes");
    rewrite(&mut value);
    serde_json::from_value(value).expect("and reads back")
}

/// 3.2: an old layout's typed panel restores with its selector, as the
/// legacy instance, and the window keys it as the panel it builds.
#[gpui_kit::test]
async fn a_layout_saved_before_identities_still_restores(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", pod("web-1", "web", 0));
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    vcx.update(|window, cx| {
        let focus = main.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx)
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("a p p = w e b enter");
    vcx.run_until_parked();
    let state = as_legacy(dump(&main, &mut vcx));
    let legacy = NavTarget::LabelLogs(LabelLogs::Typed(TypedId::LEGACY));
    let keyed: Vec<_> = super::super::panels::restored_panel_keys(&state.center)
        .into_iter()
        .flatten()
        .filter(|key| is_typed(&key.target))
        .map(|key| key.target)
        .collect();
    assert_eq!(keyed, std::slice::from_ref(&legacy));

    let restored = load(&main, state, &mut vcx);
    assert_eq!(restored.len(), 1);
    let (target, selector) = vcx.update(|_, cx| {
        let panel = restored[0].read(cx);
        (panel.label_target(), panel.test_selector())
    });
    assert_eq!(target, legacy, "built as the panel it's keyed as");
    assert_eq!(selector.as_deref(), Some("app=web"));
    wait_for_streams(&mut vcx, &restored[0], &["web-1/app#0@uid-web-1"]);
}
