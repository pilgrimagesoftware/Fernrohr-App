//! A panel's reason reaches the palette only while focus is inside the panel,
//! and not at all once the panel is gone.

use super::{reason, register};
use gpui_kit::{
    AppContext as _, Context, FocusHandle, IntoElement, ParentElement as _, Render, TestAppContext,
    VisualTestContext, Window, actions, div,
};

actions!(unavailable_test, [Blocked, Free]);

struct Host {
    panel: FocusHandle,
    elsewhere: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::InteractiveElement as _;
        div()
            .child(div().track_focus(&self.panel).child(div()))
            .child(div().track_focus(&self.elsewhere))
    }
}

fn host(cx: &mut TestAppContext) -> (gpui_kit::Entity<Host>, VisualTestContext) {
    cx.update(crate::util::test_ui::init);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            panel: cx.focus_handle(),
            elsewhere: cx.focus_handle(),
        });
        built = Some(host.clone());
        gpui_kit::component::Root::new(host, window, cx)
    });
    let host = built.expect("the window built its view");
    (host, VisualTestContext::from_window(window.into(), cx))
}

#[gpui_kit::test]
async fn a_reason_applies_only_while_focus_is_in_its_panel(cx: &mut TestAppContext) {
    let (host, mut vcx) = host(cx);
    let (panel, elsewhere) = vcx.update(|_, cx| {
        let host = host.read(cx);
        (host.panel.clone(), host.elsewhere.clone())
    });
    vcx.update(|_, cx| {
        register(
            &panel,
            |action, _| action.as_any().is::<Blocked>().then(|| "nope".into()),
            cx,
        )
    });

    vcx.update(|window, cx| window.focus(&panel, cx));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        assert_eq!(reason(&Blocked, window, cx), Some("nope".into()));
        assert_eq!(reason(&Free, window, cx), None, "only the action it names");
    });

    vcx.update(|window, cx| window.focus(&elsewhere, cx));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        assert_eq!(reason(&Blocked, window, cx), None, "focus is outside it");
    });
}

#[gpui_kit::test]
async fn a_dropped_panels_reason_no_longer_applies(cx: &mut TestAppContext) {
    let (_host, mut vcx) = host(cx);
    let gone = vcx.update(|_, cx| {
        let handle = cx.focus_handle();
        register(&handle, |_, _| Some("stale".into()), cx);
        handle.downgrade()
    });
    assert!(gone.upgrade().is_none(), "the handle was dropped");
    vcx.update(|window, cx| assert_eq!(reason(&Blocked, window, cx), None));
}
