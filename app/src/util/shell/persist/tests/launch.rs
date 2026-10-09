//! Launch brings the app to the front with its first window active.

// Named imports, for the reason `super`'s note gives.
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{WindowLayout, WorkspaceConfig, config, open_saved_or_default};
use gpui_kit::TestAppContext;

/// The first saved window is the active one once launch has opened them all,
/// not whichever happened to open last. Told apart by their saved widths.
#[gpui_kit::test]
async fn launch_activates_the_first_saved_window(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let layout = |width: f32| WindowLayout {
        width,
        height: 700.,
        ..WindowLayout::default()
    };
    config::save(
        &path,
        &WorkspaceConfig {
            namespace_defaults: Default::default(),
            sort_defaults: Default::default(),
            windows: vec![layout(900.), layout(1000.)],
        },
    )
    .unwrap();

    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        open_saved_or_default(cx, &path);
    });
    cx.run_until_parked();

    let active_width = cx.update(|cx| {
        let active = cx.active_window().expect("launch leaves a window active");
        active
            .update(cx, |_, window, _| window.bounds().size.width.as_f32())
            .unwrap()
    });
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    assert_eq!(
        active_width, 900.,
        "the first saved window is the active one"
    );

    let _ = std::fs::remove_file(&path);
}
