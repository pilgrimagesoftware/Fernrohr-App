//! Container cards expand and collapse one at a time
//! (`container-detail-expansion` 2.1), by keyboard and by mouse.

use super::config_fixture::{Harness, focus_panel, press_by_keyboard};
use super::fixtures::rich_pod;
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pod_detail::container_view::{
    container_detail_id, container_section_key, container_toggle_id,
};
use crate::k8s::resource::pod_detail::fetch::PodDetailState;
use crate::k8s::resource::pod_detail::model::{DetailSection, DetailView};
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, ElementId, Modifiers, Pixels, TestAppContext, VisualTestContext,
};
use k8s_openapi::api::core::v1::{Container, EnvVar};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

/// A panel inside a `Root` (Tab navigation is the root's), with the real
/// bindings, already holding a pod of two containers - no fetch, so the
/// connection is a stub that never connects.
fn harness(cx: &mut TestAppContext) -> Harness {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let pod = PodRef {
            namespace: "staging".into(),
            name: "api-7d9f-ftg5t".into(),
        };
        let scope = PanelScope::new(
            NavTarget::pod("staging", "api-7d9f-ftg5t"),
            "kind-dev".into(),
        );
        let panel = cx.new(|cx| {
            let mut panel =
                PodDetailPanel::with_connection(pod, scope, DetailView::Structured, connection, cx);
            panel.state = PodDetailState::Loaded(Box::new(two_container_pod()), Ok(Vec::new()));
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    Harness {
        window,
        panel: built.expect("the window built its panel"),
        config_reads: Arc::new(AtomicUsize::new(0)),
    }
}

fn two_container_pod() -> k8s_openapi::api::core::v1::Pod {
    let container = |name: &str| Container {
        name: name.into(),
        env: Some(vec![EnvVar {
            name: "LOG_LEVEL".into(),
            value: Some("debug".into()),
            ..Default::default()
        }]),
        ..Default::default()
    };
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers = vec![container("app"), container("sidecar")];
    pod
}

fn drawn(vcx: &mut VisualTestContext, h: &Harness, id: ElementId) -> bool {
    vcx.update_window(h.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .unwrap()
}

fn bounds(vcx: &mut VisualTestContext, h: &Harness, id: ElementId) -> Bounds<Pixels> {
    vcx.update_window(h.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id.clone())
            .unwrap_or_else(|| panic!("{id:?} is drawn"))
            .bounds()
    })
    .unwrap()
}

fn click(vcx: &mut VisualTestContext, h: &Harness, id: ElementId) {
    let center = vcx
        .update_window(h.window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(id.clone())
                .unwrap_or_else(|| panic!("{id:?} is drawn"))
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(center, Modifiers::none());
    vcx.run_until_parked();
}

/// Section 2.1: expanding one container draws only that container's
/// extended fields; the other card stays a summary.
#[gpui_kit::test]
async fn expanding_one_container_shows_only_its_detail(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    focus_panel(&mut vcx, &h);
    vcx.simulate_keystrokes("2");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| h.panel.read(cx).active_tab()),
        DetailSection::Containers
    );

    // Both start collapsed, each with its own control.
    assert!(drawn(&mut vcx, &h, container_toggle_id("app")));
    assert!(drawn(&mut vcx, &h, container_toggle_id("sidecar")));
    assert!(!drawn(&mut vcx, &h, container_detail_id("app")));
    assert!(!drawn(&mut vcx, &h, container_detail_id("sidecar")));

    // Tab + Space expands `app` alone.
    press_by_keyboard(&mut vcx, &h, container_toggle_id("app"));
    assert!(drawn(&mut vcx, &h, container_detail_id("app")));
    assert!(!drawn(&mut vcx, &h, container_detail_id("sidecar")));
    // Keyed by container name, apart from the label-keyed disclosures.
    vcx.update(|_, cx| {
        let open = &h.panel.read(cx).open_sections;
        assert!(open.contains(&container_section_key("app")));
        assert!(!open.contains("app"));
    });

    // A click expands `sidecar` too, and another collapses `app` again.
    click(&mut vcx, &h, container_toggle_id("sidecar"));
    assert!(drawn(&mut vcx, &h, container_detail_id("sidecar")));
    click(&mut vcx, &h, container_toggle_id("app"));
    assert!(!drawn(&mut vcx, &h, container_detail_id("app")));
    assert!(drawn(&mut vcx, &h, container_detail_id("sidecar")));
}

/// The chevron sits in a gutter of its own: the card's text, the expanded
/// detail included, starts right of it rather than running underneath.
#[gpui_kit::test]
async fn the_chevron_has_a_gutter_of_its_own(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    focus_panel(&mut vcx, &h);
    vcx.simulate_keystrokes("2");
    vcx.run_until_parked();
    click(&mut vcx, &h, container_toggle_id("app"));

    let toggle = bounds(&mut vcx, &h, container_toggle_id("app"));
    let detail = bounds(&mut vcx, &h, container_detail_id("app"));
    assert!(
        detail.left() >= toggle.right(),
        "detail starts at {:?}, inside the chevron's gutter ending {:?}",
        detail.left(),
        toggle.right()
    );
}
