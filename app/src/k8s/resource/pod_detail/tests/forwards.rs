//! `port-forward-indicators` 3-4 in the pod detail panel, over a fake cluster in a
//! real window with the panel's keymap: the forward strip comes and goes with the
//! pod's forwards, copies an address and stops a forward once confirmed; a
//! container port's start icon forwards that port with no prompt, then offers
//! copy and stop; each icon names itself in a tooltip; and Stop Port Forward
//! confirms straight away for one forward, asking which first for several.

use super::acting::{CONTEXT, Harness, open, pod};
use crate::k8s::cluster::port_forwards::{ForwardObject, PortForwards};
use crate::k8s::resource::pod_detail::forwards::{
    COPY_TOOLTIP, START_TOOLTIP, STOP_TOOLTIP, STRIP_ID, port_start_id, port_stop_id,
    strip_copy_id, strip_stop_id,
};
use crate::ui::forward_stop::{cancel_id, confirm_id, pick_button_id};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext};
use serde_json::{Value, json};

/// `shop/web-1`, running, its container `app` declaring both `ports`.
fn pod_with_ports(ports: &[u16]) -> Value {
    let mut pod = pod(true, ports[0]);
    pod["spec"]["containers"][0]["ports"] = ports
        .iter()
        .map(|port| json!({ "containerPort": port }))
        .collect();
    pod
}

impl Harness {
    fn forwarded_ports(&mut self) -> Vec<u16> {
        self.vcx.update(|_, cx| {
            PortForwards::entity(cx)
                .read(cx)
                .for_object(&ForwardObject::pod(CONTEXT, "shop", "web-1"))
                .iter()
                .map(|forward| forward.target_port)
                .collect()
        })
    }

    fn drawn(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector).is_some()
    }

    /// Clicks `id` where the current frame draws it.
    fn click(&mut self, id: impl Into<gpui_kit::SharedString>) {
        let id: &'static str = id.into().to_string().leak();
        let at = self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        });
        self.vcx.simulate_click(at, Modifiers::none());
        self.settle();
    }

    /// Runs what the last input started, and the test clock past a dialog's
    /// deferred close, before the next input.
    fn settle(&mut self) {
        self.vcx.run_until_parked();
        self.vcx
            .executor()
            .advance_clock(std::time::Duration::from_secs(1));
        self.vcx.run_until_parked();
    }

    fn key(&mut self, keys: &str) {
        self.press(keys);
        self.settle();
    }

    /// Hovers the control `id`; whether the tooltip reading `text` shows.
    fn tooltip_shows(&mut self, id: impl Into<gpui_kit::SharedString>, text: &str) -> bool {
        let id: &'static str = id.into().to_string().leak();
        let at = self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        });
        self.vcx.simulate_mouse_move(at, None, Modifiers::none());
        self.vcx
            .executor()
            .advance_clock(std::time::Duration::from_secs(1));
        self.vcx.run_until_parked();
        let selector: &'static str = crate::ui::icon_tooltip::selector(text).leak();
        self.vcx.debug_bounds(selector).is_some()
    }
}

/// 3.1: the strip shows while the pod has a forward - copy puts its address on
/// the clipboard, stop asks and then stops it - and is gone once it has none.
#[gpui_kit::test]
async fn the_strip_shows_copies_and_stops_a_forward(cx: &mut TestAppContext) {
    let mut h = open(cx, pod(true, 18_201));
    assert!(!h.drawn(STRIP_ID), "no forwards, no strip");

    h.key("shift-f");
    assert_eq!(h.forwarded_ports(), [18_201]);
    assert!(h.drawn(STRIP_ID), "the strip shows it");
    assert!(h.hint_shown("Stop forward"), "and Stop forward is a hint");

    h.click(strip_copy_id(0));
    let copied = h
        .vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(
        copied.is_some_and(|text| text.starts_with("127.0.0.1:")),
        "the local address is copied"
    );

    h.click(strip_stop_id(0));
    assert!(h.dialog_open(), "stopping asks");
    h.click(cancel_id());
    assert_eq!(h.forwarded_ports(), [18_201], "Cancel keeps it");

    h.click(strip_stop_id(0));
    h.click(confirm_id());
    assert!(h.forwarded_ports().is_empty(), "confirmed, it stops");
    assert!(!h.drawn(STRIP_ID), "and the strip goes");
}

/// 3.2: a container port's start icon forwards that port - not the first, and
/// with no prompt though the container declares two - and the port then offers
/// stop, which asks first.
#[gpui_kit::test]
async fn a_container_port_starts_and_stops_its_own_forward(cx: &mut TestAppContext) {
    let mut h = open(cx, pod_with_ports(&[18_202, 18_203]));
    h.key("2");

    h.click(port_start_id("app", 18_203));
    assert!(!h.dialog_open(), "no port prompt");
    assert_eq!(h.forwarded_ports(), [18_203], "that port");

    h.click(port_stop_id("app", 18_203));
    assert!(h.dialog_open(), "stopping asks");
    h.key("enter");
    assert!(h.forwarded_ports().is_empty(), "Enter stops it");
}

/// Every forward icon names its action in a tooltip.
#[gpui_kit::test]
async fn each_forward_icon_has_its_tooltip(cx: &mut TestAppContext) {
    let mut h = open(cx, pod_with_ports(&[18_204, 18_205]));
    h.key("2");
    assert!(h.tooltip_shows(port_start_id("app", 18_205), START_TOOLTIP));

    h.click(port_start_id("app", 18_204));
    assert!(h.tooltip_shows(strip_copy_id(0), COPY_TOOLTIP));
    assert!(h.tooltip_shows(strip_stop_id(0), STOP_TOOLTIP));
}

/// 4.1: Stop Port Forward with one forward goes straight to the confirmation;
/// with two it asks which first, and Escape anywhere keeps it.
#[gpui_kit::test]
async fn stop_port_forward_asks_which_only_when_there_are_several(cx: &mut TestAppContext) {
    let mut h = open(cx, pod_with_ports(&[18_206, 18_207]));
    h.key("2");
    h.click(port_start_id("app", 18_206));

    h.key("ctrl-shift-f");
    assert!(h.dialog_open(), "one forward: the confirmation");
    h.key("escape");
    assert_eq!(h.forwarded_ports(), [18_206], "Escape keeps it");

    h.click(port_start_id("app", 18_207));
    h.key("ctrl-shift-f");
    assert!(h.dialog_open(), "two forwards: which?");
    h.click(pick_button_id(1));
    assert!(h.dialog_open(), "then the confirmation");
    h.key("enter");
    assert_eq!(h.forwarded_ports(), [18_206], "the chosen one stopped");
}
