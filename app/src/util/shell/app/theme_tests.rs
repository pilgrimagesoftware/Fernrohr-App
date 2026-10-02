//! `toolbar-layout-with-gpui-kit` 2.1 through real windows: the status bar's theme
//! switcher and the palette command each make a theme current for every window at
//! once, and save it to `ui.toml` without touching the file's other settings.

use crate::config::ui::{TextSize, Theme as ThemePreference, UiConfig};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::theme::{UseDarkTheme, UseLightTheme};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::{Root, Theme, ThemeMode};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Modifiers, TestAppContext, VisualTestContext};

/// The app as `main` starts it on a `Light` preference saved in a temp `ui.toml`,
/// with two workspace windows open.
fn two_windows(
    cx: &mut TestAppContext,
) -> (VisualTestContext, VisualTestContext, std::path::PathBuf) {
    cx.executor().allow_parking();
    let (workspace, keymap, ui) = (
        temp_workspace_path(),
        temp_workspace_path(),
        temp_workspace_path(),
    );
    let stored = UiConfig {
        theme: ThemePreference::Light,
        text_size: TextSize::from(120),
        ..Default::default()
    };
    crate::config::save(&ui, &stored).expect("temp ui.toml written");
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::ui::theme::init(stored.theme, cx);
        crate::ui::theme::save_to(ui.clone(), cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let open = |cx: &mut TestAppContext| {
        let window = cx.add_window(|window, cx| {
            let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
            main.update(cx, |main, cx| main.focus_initial(window, cx));
            Root::new(main, window, cx)
        });
        let vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.run_until_parked();
        vcx
    };
    let first = open(cx);
    let second = open(cx);
    (first, second, ui)
}

fn mode(vcx: &mut VisualTestContext) -> ThemeMode {
    vcx.update(|_, cx| cx.global::<Theme>().mode)
}

/// The palette's Theme: Dark switches every window to dark and saves it.
#[gpui_kit::test]
async fn the_dark_command_switches_every_window_and_saves(cx: &mut TestAppContext) {
    let (mut first, mut second, ui) = two_windows(cx);
    assert_eq!(mode(&mut first), ThemeMode::Light);

    first.dispatch_action(UseDarkTheme);
    first.run_until_parked();
    assert_eq!(mode(&mut first), ThemeMode::Dark);
    assert_eq!(mode(&mut second), ThemeMode::Dark, "the other window too");

    let saved: UiConfig = crate::config::load(&ui);
    assert_eq!(saved.theme, ThemePreference::Dark, "saved to ui.toml");
    assert_eq!(
        saved.text_size,
        TextSize::from(120),
        "the rest of the file kept"
    );

    // The next launch reads it.
    let relaunch = TestAppContext::single();
    relaunch.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(saved.theme, cx);
        assert_eq!(cx.global::<Theme>().mode, ThemeMode::Dark);
    });

    second.dispatch_action(UseLightTheme);
    second.run_until_parked();
    assert_eq!(
        mode(&mut first),
        ThemeMode::Light,
        "and back, from either window"
    );
}

/// The status bar's switcher: its menu's Dark item does the same.
#[gpui_kit::test]
async fn the_status_bar_switcher_switches_the_theme(cx: &mut TestAppContext) {
    let (mut first, _second, ui) = two_windows(cx);
    let center = first.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find("status-theme-switch")
            .expect("the switcher is drawn")
            .bounds()
            .center()
    });
    first.simulate_click(center, Modifiers::none());
    first.run_until_parked();
    // System, Light, Dark: the third item.
    for key in ["down", "down", "down", "enter"] {
        first.simulate_keystrokes(key);
        first.run_until_parked();
        if mode(&mut first) == ThemeMode::Dark {
            break;
        }
    }
    assert_eq!(mode(&mut first), ThemeMode::Dark);
    let saved: UiConfig = crate::config::load(&ui);
    assert_eq!(saved.theme, ThemePreference::Dark);
}

/// The switcher is drawn wholly inside the window at its far right end - at a
/// normal width and at a narrow one, where the capsule row has to give way
/// rather than push the switcher out. (Layout wasn't what hid it on a real build:
/// its icon was missing from the asset bundle - see `crate::assets`.)
#[gpui_kit::test]
async fn the_switcher_stays_at_the_far_end_at_any_width(cx: &mut TestAppContext) {
    let (mut first, _second, _ui) = two_windows(cx);
    for width in [1200., 420.] {
        first.simulate_resize(gpui_kit::size(gpui_kit::px(width), gpui_kit::px(700.)));
        first.run_until_parked();
        let (window_bounds, switch) = first.update(|window, cx| {
            window.render_frame(cx);
            let switch = window
                .try_find("status-theme-switch")
                .expect("the switcher is drawn")
                .bounds();
            (window.bounds(), switch)
        });
        assert!(
            switch.size.width > gpui_kit::px(0.),
            "it has a size at {width}"
        );
        assert!(
            switch.right() <= window_bounds.size.width
                && switch.left() > window_bounds.size.width * 0.5,
            "at {width}px it's inside the window, at its right end: {switch:?}"
        );
    }
}
