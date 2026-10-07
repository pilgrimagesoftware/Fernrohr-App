//! `logs-panel-instancing`: a pod's own Logs panel, saved with its pod and
//! container, comes back through the real `register_restore` closure - from a
//! layout loaded into a dock - pinned to that pod, streaming that container,
//! and unmoved by later selections.

use super::super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::{NavTarget, PodRef};
use gpui_kit::component::dock::{DockAreaState, DockPlacement, DockSkin, PanelInfo, PanelState};
use gpui_kit::{Entity, TestAppContext};
use serde_json::json;
use std::rc::Rc;

struct DockHost {
    area: Entity<gpui_kit::component::dock::DockArea>,
    _skin: Rc<DockSkin>,
}

impl gpui_kit::Render for DockHost {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{ParentElement as _, Styled as _};
        gpui_kit::div().size_full().child(self.area.clone())
    }
}

#[gpui_kit::test]
async fn a_saved_pods_logs_panel_restores_pinned_to_its_pod_and_container(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::logs::register_restore(cx);
    });
    // Connected, so the restored panel starts streaming what it's pinned to.
    let (_cluster, client) = crate::k8s::test_cluster::FakeCluster::start(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client))
    });
    let (host, cx) = cx.add_window_view(|window, cx| {
        let (area, skin) = DockSkin::dock_area("logs-restore-test", Some(1), window, cx);
        DockHost { area, _skin: skin }
    });
    let area = cx.update(|_, cx| host.read(cx).area.clone());
    let saved = PanelState {
        panel_name: "Logs".to_string(),
        children: Vec::new(),
        info: PanelInfo::Panel(json!({
            "context_name": "demo",
            "namespaces": [],
            "pod_namespace": "shop",
            "pod_name": "web-1",
            "container": "proxy",
        })),
    };
    let center = PanelState {
        panel_name: "StackPanel".to_string(),
        children: vec![PanelState {
            panel_name: "TabPanel".to_string(),
            children: vec![saved],
            info: PanelInfo::Tabs { active_index: 0 },
        }],
        info: PanelInfo::Stack {
            sizes: Vec::new(),
            axis: 0,
        },
    };

    cx.update(|window, cx| {
        area.update(cx, |area, cx| {
            area.load(
                DockAreaState {
                    center,
                    ..Default::default()
                },
                window,
                cx,
            )
        })
    })
    .expect("the layout loads");
    cx.run_until_parked();

    let panel: Entity<LogsPanel> = cx.update(|_, cx| {
        let area = area.read(cx);
        let id = area
            .layout(DockPlacement::Center)
            .expect("a centre")
            .panels()
            .next()
            .expect("the restored panel");
        Entity::from(area.panel(id).expect("a built panel").as_ref())
    });
    let shows = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            let panel = panel.read(cx);
            (
                panel.scope.target.clone(),
                panel.test_pinned(),
                panel.current.clone(),
            )
        })
    };
    let pinned_to_web_1 = (
        NavTarget::PodLogs(PodRef {
            namespace: "shop".into(),
            name: "web-1".into(),
        }),
        Some(("web-1".to_string(), Some("proxy".to_string()))),
        Some(("shop".to_string(), "web-1".to_string(), "proxy".to_string())),
    );
    assert_eq!(
        shows(cx),
        pinned_to_web_1,
        "restored as web-1's own panel, streaming its saved container"
    );

    // Another pod selected afterwards - in the same context - leaves it be.
    cx.update(|_, cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "shop".into(),
            name: "web-2".into(),
            containers: vec!["web".into()],
            context_name: "demo".into(),
        })))
    });
    cx.run_until_parked();
    assert_eq!(
        shows(cx),
        pinned_to_web_1,
        "a later selection doesn't move it"
    );
}
