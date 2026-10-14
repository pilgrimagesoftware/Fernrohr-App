//! `selector-logs-context-picker`: a typed label panel in a window holding two
//! contexts switches to the other - keeping its selector and container filter,
//! in its own tab (1.1), even where another typed panel already searches it
//! (`selector-logs-multiple-panels` 2.3) - is
//! saved in its new context (1.3), and is switched from its bar's control by
//! mouse (2.2) or by its command's key (2.3), a control shown only while there
//! is a choice (2.1).

use super::{wait_for_streams, window};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::{NavTarget, OpenMode, OpenedPanel};
use crate::util::logs::{LabelLogs, LogsPanel, SwitchLabelLogsContext};
use crate::util::shell::{MainWindow, WindowMode};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ElementId, Entity, Modifiers, TestAppContext, VisualTestContext};

/// A running pod `name` labelled `app=web` in `namespace`, with containers
/// `app` and `sidecar`.
fn web_pod(namespace: &str, name: &str) -> serde_json::Value {
    let status = |container: &str| {
        serde_json::json!({
            "name": container, "ready": true, "restartCount": 0, "image": container,
            "imageID": "", "state": { "running": {} },
        })
    };
    serde_json::json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": name, "namespace": namespace, "uid": format!("uid-{name}"),
            "labels": { "app": "web" },
        },
        "status": {
            "phase": "Running",
            "containerStatuses": [status("app"), status("sidecar")],
        },
    })
}

/// A window holding `demo` - its cluster has `web-1` in `blog` - and `other`,
/// whose cluster has `web-9` in `team-b`, with the typed panel opened from
/// `demo`'s Pods list on `blog`, following `app=web`'s `app` containers.
struct TwoContexts {
    _clusters: (FakeCluster, FakeCluster),
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
    panel: Entity<LogsPanel>,
}

fn two_contexts(cx: &mut TestAppContext) -> TwoContexts {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (demo, client) = FakeCluster::start(cx);
    demo.apply("/api/v1", "pods", web_pod("blog", "web-1"));
    let (other, other_client) = FakeCluster::start(cx);
    other.apply("/api/v1", "pods", web_pod("team-b", "web-9"));
    let (main, mut vcx) = window(ConnectionState::Connected(client), cx);
    vcx.update(|window, cx| {
        ClusterRegistry::insert_test_session(cx, "other", ConnectionState::Connected(other_client));
        main.update(cx, |main, cx| {
            if let WindowMode::Workspace { contexts, .. } = &mut main.mode {
                contexts.push("other".into());
            }
            main.open_target_in(
                NavTarget::pods(),
                None,
                Some("demo".into()),
                vec!["blog".into()],
                OpenMode::Foreground,
                window,
                cx,
            );
        });
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.dispatch_action(Box::new(crate::util::logs::ShowLabelLogs), cx));
    vcx.run_until_parked();
    let panel = super::typed_panel(&main, &mut vcx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(
        &mut vcx,
        &panel,
        &["web-1/app#0@uid-web-1", "web-1/sidecar#0@uid-web-1"],
    );
    vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.carry_over(None, Some("app".into()), cx);
        })
    });
    wait_for_streams(&mut vcx, &panel, &["web-1/app#0@uid-web-1"]);
    TwoContexts {
        _clusters: (demo, other),
        main,
        vcx,
        panel,
    }
}

/// Every open typed panel: its context and namespaces, and the panel.
fn typed_panels(t: &mut TwoContexts) -> Vec<(String, Vec<String>, Entity<LogsPanel>)> {
    let main = t.main.clone();
    t.vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return Vec::new();
        };
        open_panels
            .iter()
            .filter(|open| super::is_typed(&open.key.target))
            .filter_map(|open| match &open.panel {
                Some(OpenedPanel::Logs(panel)) => Some((
                    open.key.context_name.clone(),
                    open.key.namespaces.clone(),
                    panel.clone(),
                )),
                _ => None,
            })
            .collect()
    })
}

/// Where `panel`'s tab sits in its group.
fn tab_index(t: &mut TwoContexts, panel: &Entity<LogsPanel>) -> Option<usize> {
    let main = t.main.clone();
    let id = OpenedPanel::Logs(panel.clone()).panel_id();
    t.vcx.update(|_, cx| {
        let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
            return None;
        };
        let area = dock_area.read(cx);
        let node = crate::ui::panel::tabs::group_of(area, id)?;
        crate::ui::panel::tabs::tabs_of(area, node)?
            .panels
            .iter()
            .position(|tab| *tab == id)
    })
}

/// The one typed panel in `other`, once it streams `web-9`'s `app`.
fn switched_panel(t: &mut TwoContexts) -> Entity<LogsPanel> {
    let typed = typed_panels(t);
    assert_eq!(typed.len(), 1, "one typed panel: {:?}", contexts_of(&typed));
    let (context, namespaces, panel) = typed.into_iter().next().unwrap();
    assert_eq!(context, "other");
    assert!(namespaces.is_empty(), "all namespaces: {namespaces:?}");
    wait_for_streams(&mut t.vcx, &panel, &["web-9/app#0@uid-web-9"]);
    panel
}

fn contexts_of(typed: &[(String, Vec<String>, Entity<LogsPanel>)]) -> Vec<String> {
    typed.iter().map(|(context, ..)| context.clone()).collect()
}

fn switch(t: &mut TwoContexts, context: &str) {
    let action = SwitchLabelLogsContext {
        panel: t.panel.entity_id(),
        context: context.into(),
    };
    t.vcx
        .update(|window, cx| window.dispatch_action(Box::new(action), cx));
    t.vcx.run_until_parked();
}

/// 1.1: the panel moves to `other` in its own tab, with its selector and
/// container filter over all namespaces, and nothing from `demo` streams.
#[gpui_kit::test]
async fn switching_moves_the_panel_and_what_it_follows(cx: &mut TestAppContext) {
    let mut t = two_contexts(cx);
    let old = t.panel.clone();
    let before = tab_index(&mut t, &old);
    switch(&mut t, "other");

    let panel = switched_panel(&mut t);
    let (selector, filter) = t.vcx.update(|_, cx| {
        let panel = panel.read(cx);
        (panel.test_selector(), panel.test_container_filter())
    });
    assert_eq!(selector.as_deref(), Some("app=web"));
    assert_eq!(filter.as_deref(), Some("app"));
    assert_eq!(tab_index(&mut t, &panel), before, "in the old panel's tab");
    assert_eq!(tab_index(&mut t, &old), None, "the old panel is closed");
}

/// `selector-logs-multiple-panels` 2.3: `other` already has a typed panel
/// following `app`, and switching this one there moves it anyway - in its own
/// tab, keeping `app=web` - and leaves the other as it was. Focus stays on
/// the switched panel.
#[gpui_kit::test]
async fn switching_to_a_context_with_its_own_typed_panel_moves_this_one(cx: &mut TestAppContext) {
    let mut t = two_contexts(cx);
    let main = t.main.clone();
    t.vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target_in(
                NavTarget::LabelLogs(LabelLogs::new_typed()),
                None,
                Some("other".into()),
                Vec::new(),
                OpenMode::Foreground,
                window,
                cx,
            );
        })
    });
    t.vcx.run_until_parked();
    let others = logs_panel_in(&mut t, "other");
    t.vcx.update(|_, cx| {
        others.update(cx, |panel, cx| {
            panel.carry_over(Some("app".into()), None, cx)
        })
    });
    wait_for_streams(
        &mut t.vcx,
        &others,
        &["web-9/app#0@uid-web-9", "web-9/sidecar#0@uid-web-9"],
    );
    let old = t.panel.clone();
    let before = tab_index(&mut t, &old);
    switch(&mut t, "other");

    let typed = typed_panels(&mut t);
    assert_eq!(contexts_of(&typed), ["other", "other"]);
    let switched = typed
        .iter()
        .map(|(.., panel)| panel.clone())
        .find(|panel| *panel != others)
        .expect("the switched panel");
    let selector = t.vcx.update(|_, cx| switched.read(cx).test_selector());
    assert_eq!(selector.as_deref(), Some("app=web"));
    assert_eq!(
        tab_index(&mut t, &switched),
        before,
        "in the old panel's tab"
    );
    wait_for_streams(&mut t.vcx, &switched, &["web-9/app#0@uid-web-9"]);
    wait_for_streams(
        &mut t.vcx,
        &others,
        &["web-9/app#0@uid-web-9", "web-9/sidecar#0@uid-web-9"],
    );
    let focused = t.vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        switched
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
    });
    assert!(focused, "the switched panel keeps focus");
}

fn logs_panel_in(t: &mut TwoContexts, context: &str) -> Entity<LogsPanel> {
    typed_panels(t)
        .into_iter()
        .find(|(name, ..)| name == context)
        .map(|(.., panel)| panel)
        .expect("a typed panel in that context")
}

/// 1.3: the switched panel saves its new context, selector and filter, and a
/// window holding both contexts restores it there.
#[gpui_kit::test]
async fn a_switched_panel_is_saved_in_its_new_context(cx: &mut TestAppContext) {
    let mut t = two_contexts(cx);
    switch(&mut t, "other");
    switched_panel(&mut t);
    let main = t.main.clone();
    let dock = t.vcx.update(|_, cx| {
        let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
            panic!("a workspace window");
        };
        dock_area.read(cx).dump(cx)
    });

    let mut restored = None;
    t.vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return;
            };
            dock_area.update(cx, |area, cx| {
                area.load(dock, window, cx).expect("the dump loads");
            });
            let views = main.test_dock_views(cx);
            restored = views
                .iter()
                .find_map(|view| view.view().downcast::<LogsPanel>().ok());
        })
    });
    let restored = restored.expect("a Logs panel was restored");
    let (scope, selector, filter) = t.vcx.update(|_, cx| {
        let panel = restored.read(cx);
        (
            panel.test_scope(),
            panel.test_selector(),
            panel.test_container_filter(),
        )
    });
    assert_eq!(scope, ("other".to_string(), Vec::new()));
    assert_eq!(selector.as_deref(), Some("app=web"));
    assert_eq!(filter.as_deref(), Some("app"));
}

/// Whether element `id` is drawn in the window now.
fn drawn(t: &mut TwoContexts, id: &str) -> bool {
    let id = ElementId::Name(id.to_string().into());
    t.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id.clone()).is_some()
    })
}

fn click(t: &mut TwoContexts, id: ElementId) {
    let center = t.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id.clone())
            .unwrap_or_else(|| panic!("{id:?} is drawn"))
            .bounds()
            .center()
    });
    t.vcx.simulate_click(center, Modifiers::none());
    t.vcx.run_until_parked();
}

const CONTROL: &str = "logs-label-context";

/// 2.1: the control shows with two contexts, and goes once the window holds
/// one.
#[gpui_kit::test]
async fn the_context_control_shows_only_with_a_choice(cx: &mut TestAppContext) {
    let mut t = two_contexts(cx);
    assert!(drawn(&mut t, CONTROL), "two contexts: the control shows");
    let main = t.main.clone();
    t.vcx.update(|_, cx| {
        main.update(cx, |main, cx| {
            if let WindowMode::Workspace { contexts, .. } = &mut main.mode {
                contexts.retain(|context| context != "other");
            }
            cx.notify();
        })
    });
    t.vcx.run_until_parked();
    assert!(!drawn(&mut t, CONTROL), "one context: no control");
}

/// 2.2: clicking the control and then `other` switches the panel.
#[gpui_kit::test]
async fn the_mouse_switches_from_the_control(cx: &mut TestAppContext) {
    let mut t = two_contexts(cx);
    click(&mut t, ElementId::Name(CONTROL.into()));
    // The menu's rows are drawn with their index as id: `other` is the second.
    let center = t.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within(ElementId::Name("popup-menu".into()))
            .try_find(ElementId::Integer(1))
            .expect("the menu lists `other`")
            .bounds()
            .center()
    });
    t.vcx.simulate_click(center, Modifiers::none());
    t.vcx.run_until_parked();
    switched_panel(&mut t);
}

/// 2.3: with the panel focused, `x` opens the picker; down to `other` and
/// Enter switches. Escape instead closes it with nothing switched.
#[gpui_kit::test]
async fn the_keyboard_switches_with_its_key(cx: &mut TestAppContext) {
    let mut t = two_contexts(cx);
    let focus = |t: &mut TwoContexts| {
        let panel = t.panel.clone();
        t.vcx.update(|window, cx| {
            use gpui_kit::Focusable as _;
            let handle = panel.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        });
    };
    focus(&mut t);
    t.vcx.simulate_keystrokes("x");
    t.vcx.run_until_parked();
    t.vcx.simulate_keystrokes("escape");
    t.vcx.run_until_parked();
    assert_eq!(contexts_of(&typed_panels(&mut t)), ["demo"]);

    focus(&mut t);
    t.vcx.simulate_keystrokes("x");
    t.vcx.run_until_parked();
    t.vcx.simulate_keystrokes("down enter");
    t.vcx.run_until_parked();
    switched_panel(&mut t);
}

/// 2.3: the command is in the palette, keyed `x`, and `keymap.toml` rebinds it.
#[gpui_kit::test]
async fn the_command_is_registered_and_rebindable(cx: &mut TestAppContext) {
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let command = registry
        .get("logs.pick_context")
        .expect("Logs: Pick Context is registered");
    assert_eq!(command.title, "Logs: Pick Context\u{2026}");
    assert_eq!(command.default_binding, "x");
    let mut config = crate::keymap::KeymapConfig::default();
    config
        .bindings
        .insert("logs.pick_context".into(), "ctrl-x".into());
    let bindings =
        cx.update(|cx| crate::keymap::bindings(&registry, &config, cx.keyboard_mapper().as_ref()));
    let keys: Vec<String> = bindings
        .iter()
        .filter(|binding| {
            binding
                .action()
                .partial_eq(&crate::util::logs::PickLogsContext)
        })
        .map(|binding| {
            binding
                .keystrokes()
                .iter()
                .map(|keystroke| keystroke.unparse())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(keys, ["ctrl-x"]);
}
