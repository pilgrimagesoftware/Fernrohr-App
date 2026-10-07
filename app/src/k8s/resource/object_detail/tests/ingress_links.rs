//! #157: an Ingress's host addresses open in the browser - by click, and by
//! Tab and Enter.

use super::fixtures::{kind, object, stub_panel, target};
use crate::ui::detail::url_link_id;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use serde_json::json;

/// The links' id prefix: the section, then the field.
const LINKS: &str = "Ingress/Open";

fn ingress_panel(cx: &mut TestAppContext) -> (WindowHandle<Root>, VisualTestContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let ingresses = kind("networking.k8s.io", "v1", "Ingress", true);
    let (window, panel) = stub_panel(
        cx,
        target(ingresses.clone(), Some("staging"), "web"),
        vec![ingresses],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(
                    object(json!({
                        "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                        "metadata": { "name": "web", "namespace": "staging" },
                        "spec": {
                            "rules": [
                                { "host": "app.example.com" },
                                { "host": "admin.example.com" },
                            ],
                            "tls": [{ "hosts": ["app.example.com"] }],
                        },
                    })),
                    cx,
                )
            });
            panel.read(cx).focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    (window, vcx)
}

#[gpui_kit::test]
async fn clicking_a_host_opens_it_in_the_browser(cx: &mut TestAppContext) {
    let (window, mut vcx) = ingress_panel(cx);
    let center = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(url_link_id(LINKS, 0))
                .expect("the host's link is drawn")
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(center, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(
        vcx.opened_url().as_deref(),
        Some("https://app.example.com/")
    );
}

#[gpui_kit::test]
async fn tab_and_enter_open_a_host(cx: &mut TestAppContext) {
    let (window, mut vcx) = ingress_panel(cx);
    let id = url_link_id(LINKS, 1);
    let mut focused = false;
    for _ in 0..20 {
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
        focused = vcx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.try_find(id.clone()).and_then(|link| link.focused()) == Some(true)
            })
            .unwrap();
        if focused {
            break;
        }
    }
    assert!(focused, "Tab reaches the second host's link");
    // A key down *and* up, as a real press is: the click fires on release.
    let enter = gpui_kit::Keystroke::parse("enter").unwrap();
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: enter });
    vcx.run_until_parked();
    assert_eq!(
        vcx.opened_url().as_deref(),
        Some("http://admin.example.com/")
    );
}
