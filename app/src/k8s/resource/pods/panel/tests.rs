// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::resource::pods::{DescribePod, PodSelection, PodsPanel, SelectedPod, ShowPodYaml};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::{AppContext as _, InteractiveElement as _, ParentElement as _, Styled as _};
use kube_runtime::watcher;

use crate::k8s::resource::pods::test_support::*;

struct PanelHarness {
    first: gpui_kit::Entity<PodsPanel>,
    second: gpui_kit::Entity<PodsPanel>,
    /// Counts the app-level detail requests that bubble past both panels
    /// to the window root - i.e. the requests the panels actually emitted,
    /// kept apart by view so a test can tell `d` from `y`.
    dispatches: std::rc::Rc<std::cell::RefCell<Vec<DetailView>>>,
}

impl gpui_kit::Render for PanelHarness {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        let dispatches = self.dispatches.clone();
        let structured = dispatches.clone();
        let yaml = dispatches.clone();
        gpui_kit::div()
            .size_full()
            .on_action(move |_: &crate::ui::nav::ShowPodDetail, _window, _cx| {
                structured.borrow_mut().push(DetailView::Structured);
            })
            .on_action(move |_: &crate::ui::nav::ShowPodDetailYaml, _window, _cx| {
                yaml.borrow_mut().push(DetailView::Yaml);
            })
            .child(self.first.clone())
            .child(self.second.clone())
    }
}

/// `d`/`y` are panel-scoped: only the focused panel's handler runs, and it
/// forwards an app-level detail request to the window. This pins all three
/// halves - the focused panel asks once, it asks only when a pod is
/// selected, and each shortcut asks for its own view rather than both
/// asking for the same one.
#[gpui_kit::test]
async fn pod_shortcut_dispatches_only_to_the_focused_panel(cx: &mut gpui_kit::TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let dispatches = Rc::new(RefCell::new(Vec::new()));
    let (first, second) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "dev".into(),
        })));
        let first = cx
            .new(|cx| PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "dev".into()), cx));
        let second = cx
            .new(|cx| PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "prod".into()), cx));
        (first, second)
    });
    let window = cx.add_window({
        let dispatches = dispatches.clone();
        let harness_first = first.clone();
        let harness_second = second.clone();
        move |_, _| PanelHarness {
            first: harness_first.clone(),
            second: harness_second.clone(),
            dispatches: dispatches.clone(),
        }
    });
    cx.run_until_parked();

    window
        .update(cx, |_, window, cx| {
            let focus_handle = first.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
            window.dispatch_action(Box::new(ShowPodYaml), cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        *dispatches.borrow(),
        vec![DetailView::Yaml],
        "only the focused panel forwards the request, and `y` asks for the \
         YAML rather than the panel's default view"
    );

    // `d` is the same panel by a different route, so it asks for the field
    // list. If both shortcuts emitted one action there would be nothing
    // left to distinguish them at the far end.
    dispatches.borrow_mut().clear();
    window
        .update(cx, |_, window, cx| {
            let focus_handle = first.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
            window.dispatch_action(Box::new(DescribePod), cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        *dispatches.borrow(),
        vec![DetailView::Structured],
        "`d` asks for the structured view"
    );

    // No selection means nothing to open, so the shortcut is inert rather
    // than opening a detail panel for whatever was selected last.
    cx.update(|cx| cx.set_global(SelectedPod(None)));
    dispatches.borrow_mut().clear();
    window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(ShowPodYaml), cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert!(
        dispatches.borrow().is_empty(),
        "an unselected pod opens nothing"
    );
}

/// The shortcuts reach the focused panel by real keystrokes, through the
/// bindings the app builds from the command registry - not a hand-kept
/// binding list, which is what these commands replaced.
#[gpui_kit::test]
async fn typed_pod_shortcuts_reach_the_focused_panel(cx: &mut gpui_kit::TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let dispatches = Rc::new(RefCell::new(Vec::new()));
    let logs_requests = Rc::new(RefCell::new(0));
    let first = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = crate::command::CommandRegistry::new();
        super::register_commands(&mut registry);
        cx.bind_keys(crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            &gpui_kit::DummyKeyboardMapper,
        ));
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "dev".into(),
        })));
        cx.new(|cx| PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "dev".into()), cx))
    });
    let second = cx.update(|cx| {
        cx.new(|cx| PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "prod".into()), cx))
    });
    let window = cx.add_window({
        let dispatches = dispatches.clone();
        let (first, second) = (first.clone(), second.clone());
        move |_, _| PanelHarness {
            first: first.clone(),
            second: second.clone(),
            dispatches: dispatches.clone(),
        }
    });
    cx.update(|cx| {
        let logs_requests = logs_requests.clone();
        cx.on_action(move |_: &crate::ui::nav::ShowLogs, _cx| {
            *logs_requests.borrow_mut() += 1;
        });
    });
    let cx = &mut gpui_kit::VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        let handle = first.read(cx).focus_handle.clone();
        handle.focus(window, cx);
    });

    cx.simulate_keystrokes("d y");
    cx.run_until_parked();
    assert_eq!(
        *dispatches.borrow(),
        vec![DetailView::Structured, DetailView::Yaml],
        "`d` asks for the fields and `y` for the YAML"
    );

    cx.simulate_keystrokes("l");
    cx.run_until_parked();
    assert_eq!(*logs_requests.borrow(), 1, "`l` asks for the pod's logs");
}

/// `connection-status-bar` task 3.1: a paused watch no longer prints anything of its
/// own in the Pods panel - the panel keeps showing its last rows regardless, and it's
/// the window's status bar that now reports the pause. Proven end to end: a real
/// pause through `ClusterRegistry::apply_health_transition` (the same entry point a
/// tunnel flap or a 401 drives in production), then both sides checked - the panel's
/// rows are still there, and a `StatusBarView` for the same context reports `Paused`.
#[gpui_kit::test]
async fn a_paused_context_still_shows_rows_and_the_bar_reports_the_pause(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::k8s::cluster::health::HealthTransition;
    use crate::k8s::cluster::namespaces::NamespaceList;
    use crate::k8s::cluster::session::ClusterRegistry;
    use crate::k8s::cluster::watch_registry::PauseReason;
    use crate::ui::status_bar::StatusBarView;

    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    let context_name = "status-bar-paused";
    // Registered up front with a fixed `Connected` state: otherwise the registry's
    // lookup below starts a real connect for this fixture context, which fails and
    // (failed outranking paused) turns the health `Failed` whenever it lands first.
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            context_name,
            ConnectionState::Connected(client.clone()),
        )
    });
    // A bare entity, not a window: `PodsPanel::render`'s `Connected` branch draws a
    // real `DataTable`, and driving that through an actual window paint pass is its
    // own can of worms unrelated to this test - every other `Connected`-branch check
    // in this module reads `self.table`'s data directly instead, and this follows
    // that convention.
    let panel = cx.update(|cx| {
        cx.new(|cx| {
            let connection =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            let namespaces = cx.new(|_| NamespaceList::empty());
            PodsPanel::with_connection(
                PanelScope::new(NavTarget::pods(), context_name.to_string()),
                connection,
                namespaces,
                cx,
            )
        })
    });
    cx.run_until_parked();

    cx.update(|cx| {
        let table = panel.read(cx).table.clone();
        table.update(cx, |table, cx| {
            table.apply(watcher::Event::Init);
            table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
            table.apply(watcher::Event::InitDone);
            cx.notify();
        });
    });
    cx.run_until_parked();

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            context_name,
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    cx.run_until_parked();

    cx.update(|cx| {
        assert_eq!(
            panel.read(cx).table.read(cx).pods().len(),
            1,
            "a paused watch keeps showing its last rows"
        );
    });

    let bar = cx.update(|cx| cx.new(|cx| StatusBarView::new(vec![context_name.to_string()], cx)));
    cx.update(|cx| {
        let items = bar.read(cx).items(cx);
        assert_eq!(items.len(), 1);
        assert!(
            matches!(
                items[0].health,
                crate::k8s::cluster::context_health::ContextHealth::Paused {
                    reason: PauseReason::Reconnecting,
                    ..
                }
            ),
            "the status bar, not the panel, now reports the pause"
        );
    });

    // `context_name` stays paused for the rest of the test, so `bar`'s tick would
    // otherwise keep re-arming its timer forever (`StatusBarView::ensure_tick`'s own
    // doc comment) instead of the test harness's end-of-test drain ever finishing.
    drop(bar);
    cx.run_until_parked();
}
