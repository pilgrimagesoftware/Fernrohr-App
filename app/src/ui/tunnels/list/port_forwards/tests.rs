//! `port-forward-indicators` 5.1-5.2 in the Tunnels window: the divider above the
//! Port forwards section, with or without a forward; and Stop asking first -
//! Cancel or Escape keeping the forward, Enter or the confirm button stopping it -
//! with each dialog button's key bound.

// NAMED imports only (no `use super::*;`): a glob of `gpui_kit::*` next to
// `#[gpui_kit::test]` shadows the builtin `#[test]` and blows the macro-expansion budget.
use super::{DIVIDER_ID, STOP_TOOLTIP, stop_button_id};
use crate::k8s::cluster::port_forwards::{ForwardObject, PortForwardRequest, PortForwards};
use crate::ui::forward_stop::{cancel_id, confirm_id};
use crate::ui::tunnels::list::TunnelsWindow;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};

struct Harness {
    vcx: VisualTestContext,
    forwards: Entity<PortForwards>,
}

fn request(port: u16) -> PortForwardRequest {
    PortForwardRequest {
        context_name: "demo".into(),
        namespace: "shop".into(),
        pod: "web-1".into(),
        remote_port: port,
    }
}

/// A Tunnels window, under a `Root` for its dialogs, over a fake cluster with a
/// Running `web-1`, forwarding each of `ports`.
fn harness(cx: &mut TestAppContext, ports: &[u16]) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = crate::k8s::test_cluster::FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "pods",
        serde_json::json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
            "spec": { "containers": [{ "name": "app", "image": "nginx" }] },
            "status": { "phase": "Running" } }),
    );
    let forwards = cx.update(PortForwards::entity);
    for &port in ports {
        forwards
            .update(cx, |forwards, cx| {
                let origin = ForwardObject::pod("demo", "shop", "web-1");
                forwards.start(request(port), origin, client.clone(), cx)
            })
            .expect("started");
    }
    let tunnels = crate::util::test_paths::temp_path("tunnels-port-forwards");
    let kubeconfig = std::env::temp_dir().join("fernrohr-port-forwards-no-such-kubeconfig.yaml");
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|cx| TunnelsWindow::new(tunnels, Some(kubeconfig), window, cx));
        Root::new(view, window, cx)
    });
    let vcx = VisualTestContext::from_window(window.into(), cx);
    Harness { vcx, forwards }
}

impl Harness {
    fn listed(&mut self) -> usize {
        self.forwards
            .read_with(&self.vcx, |forwards, _| forwards.list().len())
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn click(&mut self, id: &'static str) {
        let at = self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        });
        self.vcx.simulate_click(at, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn click_stop(&mut self) {
        self.click(Box::leak(stop_button_id(0).to_string().into_boxed_str()));
        assert!(self.dialog_open(), "Stop asks first");
    }

    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }
}

#[gpui_kit::test]
async fn the_divider_sits_above_the_section_with_or_without_forwards(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[]);
    h.vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        h.vcx.debug_bounds(DIVIDER_ID).is_some(),
        "with none running"
    );

    let mut h = harness(cx, &[18_085]);
    h.vcx.update(|window, cx| window.render_frame(cx));
    assert!(h.vcx.debug_bounds(DIVIDER_ID).is_some(), "with one running");
}

/// The Stop icon names its action in a tooltip (`.claude/rules/icon-buttons.md`).
#[gpui_kit::test]
async fn the_stop_icon_has_its_tooltip(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[18_086]);
    let id: &'static str = Box::leak(stop_button_id(0).to_string().into_boxed_str());
    let at = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).bounds().center()
    });
    h.vcx.simulate_mouse_move(at, None, Modifiers::none());
    h.vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(1));
    h.vcx.run_until_parked();
    let selector: &'static str = crate::ui::icon_tooltip::selector(STOP_TOOLTIP).leak();
    assert!(h.vcx.debug_bounds(selector).is_some(), "the tooltip shows");
}

#[gpui_kit::test]
async fn cancel_and_escape_keep_the_forward(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[18_087]);

    h.click_stop();
    h.click(cancel_id().to_string().leak());
    assert!(!h.dialog_open());
    assert_eq!(h.listed(), 1, "Cancel keeps it");

    h.click_stop();
    h.press("escape");
    assert!(!h.dialog_open());
    assert_eq!(h.listed(), 1, "Escape keeps it");
}

#[gpui_kit::test]
async fn enter_or_the_confirm_button_stops_it(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[18_088, 18_089]);
    let keys_bound = h.vcx.update(|window, _| {
        use gpui_kit::base::actions::{Cancel, Confirm};
        use gpui_kit::component::kbd::Kbd;
        Kbd::binding_for_action(&Cancel, Some("Dialog"), window).is_some()
            && Kbd::binding_for_action(&Confirm { secondary: false }, Some("Dialog"), window)
                .is_some()
    });
    assert!(keys_bound, "both buttons have a key to show");

    h.click_stop();
    h.press("enter");
    assert!(!h.dialog_open());
    assert_eq!(h.listed(), 1, "Enter stopped one");

    h.click_stop();
    h.click(confirm_id().to_string().leak());
    assert_eq!(h.listed(), 0, "the confirm button stopped the other");
}
