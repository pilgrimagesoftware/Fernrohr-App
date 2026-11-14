//! `helm-releases` 7.1: the Tools section's helm path, set from the keyboard
//! alone - Previous Tab reaches the section, which focuses the field; typing
//! and Enter save the path and check `helm` again.

use super::{app, open, press};
use crate::helm_cli::{HelmBinary, HelmStatus};
use crate::ui::settings::tools::HELM_STATUS;
use crate::ui::settings::{Section, SettingsWindow};
use gpui_kit::{TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn the_helm_path_is_set_from_the_keyboard(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, _shortcuts) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    // Tools is the last tab: one step back from the first.
    press(&mut vcx, "ctrl-shift-tab");
    let shown = handle
        .update(cx, |root, _window, cx| {
            root.view()
                .clone()
                .downcast::<SettingsWindow>()
                .unwrap()
                .read(cx)
                .section()
        })
        .unwrap();
    assert_eq!(shown, Section::Tools);

    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.simulate_input("/opt/helm/bin/helm");
    press(&mut vcx, "enter");
    cx.update(|cx| {
        assert_eq!(
            crate::helm_cli::current_path(cx).as_deref(),
            Some("/opt/helm/bin/helm")
        );
        assert_eq!(
            crate::helm_cli::requested(cx),
            Some("/opt/helm/bin/helm".into()),
            "Enter checks the new path"
        );
    });

    // The section says what the check found.
    cx.update(|cx| {
        crate::helm_cli::set_status(
            HelmStatus::Ready(HelmBinary {
                path: "/opt/helm/bin/helm".into(),
                version: "v3.14.2".into(),
            }),
            cx,
        )
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    let _ = vcx.update(|window, cx| window.draw(cx));
    assert!(
        vcx.debug_bounds(HELM_STATUS).is_some(),
        "the status line is drawn"
    );
}
