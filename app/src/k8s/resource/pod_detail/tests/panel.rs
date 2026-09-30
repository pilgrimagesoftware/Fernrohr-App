//! The panel in a window: views, tabs, keyboard shortcuts and the missing-pod
//! state.

use super::fixtures::{field, rich_pod};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pod_detail::commands::PANEL_KEY_CONTEXT;
use crate::k8s::resource::pod_detail::fetch::PodDetailState;
use crate::k8s::resource::pod_detail::model::{DetailSection, DetailView, PodFieldValue};
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::keymap::KeymapConfig;
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use jiff::Timestamp;

/// The panel's bindings as the app builds them: from its registered commands,
/// through `keymap::bindings`, with `keymap`'s overrides.
fn registered_bindings(keymap: &KeymapConfig, cx: &gpui_kit::App) -> Vec<gpui_kit::KeyBinding> {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    crate::keymap::bindings(&registry, keymap, cx.keyboard_mapper().as_ref())
}

pub(super) fn stub_panel(
    cx: &mut TestAppContext,
    state: ConnectionState,
) -> gpui_kit::WindowHandle<PodDetailPanel> {
    stub_panel_viewing(cx, state, DetailView::Structured)
}

/// The same panel, opened on a specific view - so a test can cover the
/// `y` path (which opens straight into YAML) without a live cluster.
fn stub_panel_viewing(
    cx: &mut TestAppContext,
    state: ConnectionState,
    view: DetailView,
) -> gpui_kit::WindowHandle<PodDetailPanel> {
    let connection = cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(state)));
    cx.add_window(|_window, cx| {
        let pod = PodRef {
            namespace: "staging".into(),
            name: "api-7d9f-ftg5t".into(),
        };
        let scope = PanelScope::new(
            NavTarget::pod("staging", "api-7d9f-ftg5t"),
            "kind-dev".into(),
        );
        PodDetailPanel::with_connection(pod, scope, view, connection, cx)
    })
}

/// `nav.show_pod_detail_yaml` asks for the panel on the YAML, so the view
/// the panel opens on is a construction argument rather than a constant.
/// The other half - that the same panel switches view when it is already
/// open - is `MainWindow`'s, and is tested there.
#[gpui_kit::test]
async fn a_panel_can_be_built_opening_on_the_yaml(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel_viewing(cx, ConnectionState::Connecting, DetailView::Yaml);

    window
        .update(cx, |panel, _window, _cx| {
            assert_eq!(panel.viewing, DetailView::Yaml);
        })
        .unwrap();

    // And it is a view, not a mode: the toggle still moves between the two.
    window
        .update(cx, |panel, _window, cx| {
            panel.set_view(DetailView::Structured, cx);
            assert_eq!(panel.viewing, DetailView::Structured);
        })
        .unwrap();
}

/// Section 4.1: the structured field list is the panel's default view, and
/// it is the projection of the pod it holds - not a text blob.
#[gpui_kit::test]
async fn the_panel_renders_the_structured_field_list_by_default(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx, ConnectionState::Connecting);

    window
        .update(cx, |panel, _window, cx| {
            panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |panel, _window, _cx| {
            assert_eq!(panel.viewing, DetailView::Structured);
            let fields = panel.fields(Timestamp::from_second(90).unwrap());
            assert!(field(&fields, "Name").is_some());
            assert!(field(&fields, "Conditions").is_some());
            assert!(
                matches!(
                    field(&fields, "Labels").unwrap().value,
                    PodFieldValue::Chips(_)
                ),
                "labels are chips, not one run-on line"
            );
            assert!(
                matches!(
                    field(&fields, "Conditions").unwrap().value,
                    PodFieldValue::Badges(_)
                ),
                "conditions are badges"
            );
            assert!(
                matches!(
                    field(&fields, "Managed Fields").unwrap().value,
                    PodFieldValue::ManagedFields(_)
                ),
                "managed fields render as per-manager blocks"
            );
        })
        .unwrap();
    // The render itself runs as part of the window's frame, so a panic in
    // it would fail here rather than being silently skipped.
    cx.run_until_parked();
}

/// Section 4.3: the toolbar toggle swaps the field list for the raw YAML
/// and back, without leaving the panel.
#[gpui_kit::test]
async fn the_toolbar_toggles_between_fields_and_yaml(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx, ConnectionState::Connecting);

    window
        .update(cx, |panel, _window, cx| {
            panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
            panel.set_view(DetailView::Yaml, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |panel, _window, _cx| {
            assert_eq!(panel.viewing, DetailView::Yaml);
            let yaml = panel.yaml().expect("a loaded pod renders as YAML");
            assert!(yaml.contains("api-7d9f-ftg5t"), "the manifest is the pod's");
        })
        .unwrap();
}

/// Section 2.2: switching tabs shows only that tab's fields - the other
/// tabs' fields are gone from the rendered set, not merely reordered.
#[gpui_kit::test]
async fn switching_tabs_shows_only_that_tabs_fields(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx, ConnectionState::Connecting);

    window
        .update(cx, |panel, _window, cx| {
            panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |panel, _window, cx| {
            assert_eq!(panel.active_tab(), DetailSection::Overview);
            let all = panel.fields(Timestamp::from_second(90).unwrap());
            let overview_only: Vec<&str> = all
                .iter()
                .filter(|f| f.section == DetailSection::Overview)
                .map(|f| f.label)
                .collect();
            assert!(overview_only.contains(&"Name"));
            assert!(overview_only.contains(&"Conditions"));
            assert!(!overview_only.contains(&"Containers"));

            panel.set_active_tab(DetailSection::Containers, cx);
            let containers_only: Vec<&str> = all
                .iter()
                .filter(|f| f.section == panel.active_tab())
                .map(|f| f.label)
                .collect();
            assert!(containers_only.contains(&"Containers"));
            assert!(!containers_only.contains(&"Name"));
        })
        .unwrap();
}

/// Section 2.3: the tab keys are not only printed in the hint bar - real
/// keystrokes, through the panel's key context and the bound keymap, move
/// the active tab. `y` then toggles to YAML and back, keeping the tab.
#[gpui_kit::test]
async fn the_tab_keys_switch_tabs_from_the_keyboard(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        cx.bind_keys(registered_bindings(&KeymapConfig::default(), cx));
    });
    let window = stub_panel(cx, ConnectionState::Connecting);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);

    window
        .update(&mut vcx, |panel, window, cx| {
            panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    vcx.run_until_parked();

    let active_tab = |vcx: &mut VisualTestContext| {
        window
            .update(vcx, |panel, _window, _cx| panel.active_tab())
            .unwrap()
    };
    assert_eq!(active_tab(&mut vcx), DetailSection::Overview);

    // Every tab, not just one: each has its own action, and an action with
    // no `on_action` listener fails silently rather than to compile. Ends
    // back on Overview so its key is exercised from another tab.
    for (keystroke, expected) in [
        ("2", DetailSection::Containers),
        ("3", DetailSection::Configuration),
        ("4", DetailSection::Volumes),
        ("5", DetailSection::Events),
        ("6", DetailSection::ManagedFields),
        ("1", DetailSection::Overview),
    ] {
        vcx.simulate_keystrokes(keystroke);
        vcx.run_until_parked();
        assert_eq!(active_tab(&mut vcx), expected, "after pressing {keystroke}");
    }

    vcx.simulate_keystrokes("4 y");
    vcx.run_until_parked();
    let view = |vcx: &mut VisualTestContext| {
        window
            .update(vcx, |panel, _window, _cx| panel.view())
            .unwrap()
    };
    assert_eq!(view(&mut vcx), DetailView::Yaml);
    vcx.simulate_keystrokes("y");
    vcx.run_until_parked();
    assert_eq!(view(&mut vcx), DetailView::Structured);
    assert_eq!(
        active_tab(&mut vcx),
        DetailSection::Volumes,
        "returning from YAML lands on the tab the user left"
    );
}

/// Every panel shortcut is a registry command gated to this panel's key
/// context, so it has a palette entry and a keymap id - and a
/// `keymap.toml` override rebinds the real key.
#[gpui_kit::test]
async fn the_panel_shortcuts_are_context_gated_commands(cx: &mut TestAppContext) {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let commands: Vec<_> = registry.iter().collect();
    assert_eq!(
        commands.len(),
        8,
        "the view toggle, six tabs, and Hide Secret Values"
    );
    assert!(
        commands
            .iter()
            .all(|command| command.context == Some(PANEL_KEY_CONTEXT) && command.menu.is_none()),
        "panel shortcuts are panel-scoped and stay out of the menu bar"
    );
    assert!(registry.available(&[]).is_empty());
    assert_eq!(registry.available(&[PANEL_KEY_CONTEXT]).len(), 8);

    let mut keymap = KeymapConfig::default();
    keymap
        .bindings
        .insert("pod_detail.tab_events".into(), "e".into());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        cx.bind_keys(registered_bindings(&keymap, cx));
    });
    let window = stub_panel(cx, ConnectionState::Connecting);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |panel, window, cx| {
            panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    vcx.run_until_parked();

    vcx.simulate_keystrokes("e");
    vcx.run_until_parked();
    assert_eq!(
        window
            .update(&mut vcx, |panel, _window, _cx| panel.active_tab())
            .unwrap(),
        DetailSection::Events,
        "the override key reaches the Events tab"
    );
}

/// Section 4.4: a pod that is gone is reported as gone - its own state, not
/// an error, and not a closed panel.
#[gpui_kit::test]
async fn a_missing_pod_shows_that_it_no_longer_exists(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx, ConnectionState::Connecting);

    window
        .update(cx, |panel, _window, cx| {
            panel.state = PodDetailState::NotFound;
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |panel, _window, _cx| {
            assert!(matches!(panel.state, PodDetailState::NotFound));
            assert!(panel.pod().is_none());
            assert!(panel.fields(Timestamp::now()).is_empty());
        })
        .unwrap();
    cx.run_until_parked();
}
