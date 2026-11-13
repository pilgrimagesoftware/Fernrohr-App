//! 7.1: the Settings path for `helm` - saved to `ui.toml` alone, blank
//! meaning the login shell's PATH, `~` expanded - and checked again on change.

use super::{current, init, set_path};
use crate::config::{self, ui::UiConfig};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn setting_the_path_saves_it_and_checks_again(cx: &mut TestAppContext) {
    let file = crate::util::test_paths::temp_path("helm-preference");
    config::save(
        &file,
        &UiConfig {
            shortcut_timeout_secs: config::ui::ShortcutTimeout::from(7),
            ..UiConfig::default()
        },
    )
    .unwrap();
    cx.update(|cx| {
        init(None, file.clone(), cx);
        assert_eq!(
            crate::helm_cli::locate::requested(cx),
            None,
            "the PATH's helm"
        );

        set_path("  /opt/helm/bin/helm  ", cx);
        assert_eq!(
            current(cx).as_deref(),
            Some("/opt/helm/bin/helm"),
            "trimmed"
        );
        assert_eq!(
            crate::helm_cli::locate::requested(cx),
            Some("/opt/helm/bin/helm".into()),
            "checked again at the new path"
        );
    });
    let saved: UiConfig = config::load(&file);
    assert_eq!(saved.helm_path.as_deref(), Some("/opt/helm/bin/helm"));
    assert_eq!(
        saved.shortcut_timeout_secs,
        config::ui::ShortcutTimeout::from(7),
        "other settings kept"
    );

    cx.update(|cx| {
        set_path("~/bin/helm", cx);
        let home = dirs::home_dir().expect("a home dir");
        assert_eq!(
            crate::helm_cli::locate::requested(cx),
            Some(home.join("bin/helm"))
        );
        set_path("", cx);
        assert_eq!(current(cx), None, "blank means the PATH's helm");
        assert_eq!(crate::helm_cli::locate::requested(cx), None);
    });
    let saved: UiConfig = config::load(&file);
    assert_eq!(saved.helm_path, None);
}
