//! The spec's "Uninstall from the list": select `web`, Uninstall by its key,
//! confirm with the deliberate shortcut - and how many objects the
//! confirmation counts.

use super::fixture::{CONTEXT, expected, normalized, start, wait_until};
use crate::k8s::resource::helm::list::HelmReleasesPanel;
use crate::ui::confirm_dialog::deliver_first_frame;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Focusable as _, TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn uninstall_from_the_list_by_keys(cx: &mut TestAppContext) {
    let (_cluster, helm) = start(cx, "s8-uninstall-list", "exit 0");
    let scope = PanelScope::new(NavTarget::HelmReleases, CONTEXT.into());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| HelmReleasesPanel::new(scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let focus = panel.clone();
    vcx.update(|window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
    let listed = panel.clone();
    wait_until(&mut vcx, "listed web", |vcx| {
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
            !listed.read(cx).displayed(cx).is_empty()
        })
    });

    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();
    let selected = panel.clone();
    assert_eq!(
        vcx.update(|_, cx| selected.read(cx).selected_release(cx)),
        Some("web".into())
    );
    vcx.simulate_keystrokes("ctrl-d");
    wait_until(&mut vcx, "asked", |vcx| {
        vcx.update(|window, cx| window.has_active_dialog(cx))
    });
    deliver_first_frame(&mut vcx);
    vcx.simulate_keystrokes("secondary-backspace");
    let seen = helm.seen.clone();
    wait_until(&mut vcx, "ran helm", |_| seen.exists());
    assert_eq!(
        normalized(helm.args().expect("helm ran")),
        expected("uninstall", &["web"])
    );
}

/// The confirmation's count: the deployed revision's manifest, two objects.
#[gpui_kit::test]
async fn the_deployed_manifests_objects_are_counted(cx: &mut TestAppContext) {
    use crate::k8s::cluster::connection::ConnectionState;
    use crate::k8s::cluster::session::ClusterRegistry;
    let (_cluster, _helm) = start(cx, "s8-count", "exit 0");
    let client = cx.update(|cx| {
        let connection = ClusterRegistry::existing_connection(cx, CONTEXT).unwrap();
        match &connection.read(cx).state {
            ConnectionState::Connected(client) => client.clone(),
            _ => unreachable!("the fixture connects"),
        }
    });
    let counted = cx.update(|cx| {
        super::super::manifest::fetch_count(
            client,
            "shop".into(),
            "sh.helm.release.v1.web.v3".into(),
            cx,
        )
    });
    assert_eq!(counted.await, Some(2));
}
