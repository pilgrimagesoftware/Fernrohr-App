//! helm-releases 3.1: the Helm Releases list opened from the keyboard alone,
//! through the command palette, in a connected window with the app's keymap.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, NavTarget, TOGGLE_PALETTE_DEFAULT_BINDING, WindowMode, init};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Keystroke, TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn the_palette_opens_helm_releases(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let palette = Keystroke::parse(TOGGLE_PALETTE_DEFAULT_BINDING)
        .expect("a valid keystroke")
        .unparse();
    vcx.simulate_keystrokes(&palette);
    vcx.run_until_parked();
    assert!(
        vcx.update(|window, cx| window.has_active_dialog(cx)),
        "the palette opened"
    );
    vcx.simulate_input("Helm Releases");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    let opened = vcx.update(|_, cx| match &main.read(cx).mode {
        WindowMode::Workspace { open_panels, .. } => open_panels
            .iter()
            .any(|open| open.key.target == NavTarget::HelmReleases),
        WindowMode::Picker(_) => false,
    });
    assert!(
        opened,
        "Enter on the palette's Helm Releases opened the list"
    );
}

/// `helm-releases` "Reading a release without the mouse": Helm Releases from
/// the palette, Down to the row, Enter, then the History tab's key.
#[gpui_kit::test]
async fn a_release_reads_without_the_mouse(cx: &mut TestAppContext) {
    use crate::k8s::resource::helm::ReleaseKey;
    use crate::k8s::test_cluster::FakeCluster;
    use base64::Engine as _;
    use std::io::Write as _;

    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
    });
    let (cluster, client) = FakeCluster::start(cx);
    for (version, status) in [(1, "superseded"), (2, "deployed")] {
        let release = serde_json::json!({ "name": "web", "namespace": "shop", "version": version,
            "info": { "status": status }, "chart": { "metadata": { "name": "web", "version": "1.0.0" } } });
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(release.to_string().as_bytes()).unwrap();
        let helm = base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap());
        cluster.apply("/api/v1", "secrets", serde_json::json!({
            "apiVersion": "v1", "kind": "Secret", "type": "helm.sh/release.v1",
            "metadata": { "name": format!("sh.helm.release.v1.web.v{version}"), "namespace": "shop",
                "labels": { "owner": "helm", "name": "web", "version": version.to_string(), "status": status } },
            "data": { "release": base64::engine::general_purpose::STANDARD.encode(helm) },
        }));
    }
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let palette = Keystroke::parse(TOGGLE_PALETTE_DEFAULT_BINDING)
        .expect("a valid keystroke")
        .unparse();
    vcx.simulate_keystrokes(&palette);
    vcx.simulate_input("Helm Releases");
    vcx.simulate_keystrokes("enter");
    // The list's rows arrive off the UI thread: wait until it shows one.
    let mut listed = false;
    for _ in 0..400 {
        vcx.run_until_parked();
        vcx.update(gpui_kit::test::TestWindowExt::render_frame);
        listed = vcx.update(|_, cx| match &main.read(cx).mode {
            WindowMode::Workspace { open_panels, .. } => open_panels.iter().any(|open| {
                matches!(&open.panel, Some(crate::ui::nav::OpenedPanel::HelmReleases(panel))
                    if !panel.read(cx).displayed(cx).is_empty())
            }),
            WindowMode::Picker(_) => false,
        });
        if listed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(listed, "the palette opened the list, and it listed web");
    vcx.simulate_keystrokes("down");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("2");
    vcx.run_until_parked();

    let release = ReleaseKey {
        namespace: "shop".into(),
        name: "web".into(),
    };
    let tab = vcx.update(|_, cx| match &main.read(cx).mode {
        WindowMode::Workspace { open_panels, .. } => {
            open_panels
                .iter()
                .find_map(|open| match (&open.key.target, &open.panel) {
                    (
                        NavTarget::HelmRelease(key),
                        Some(crate::ui::nav::OpenedPanel::HelmRelease(panel)),
                    ) if *key == release => Some(panel.read(cx).tab),
                    _ => None,
                })
        }
        WindowMode::Picker(_) => None,
    });
    assert_eq!(
        tab,
        Some(crate::k8s::resource::helm::detail::ReleaseTab::History),
        "Enter opened web's panel, and its History key showed History"
    );
}
