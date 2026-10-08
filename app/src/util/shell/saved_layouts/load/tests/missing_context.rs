//! Section 5.1: a saved panel scoped to a context this window doesn't hold
//! restores as `ui::unrestored`'s placeholder - naming the context, in the
//! same slot - for both Replace and Add, rather than silently connecting it
//! (design.md D5). `ClusterRegistry::connection`'s `ensure_init` connects a
//! context lazily on first use, so building that panel's own kind here would
//! be exactly the "connect behind the user's back" the design forbids;
//! `open_target_in`'s own refusal (section 5's write-up) is what already
//! stops that for every other open, and `saved_layouts::load` (`load.rs`)
//! gives it the same refusal plus a placeholder instead of nothing.
//!
//! Named imports from `super`, not `use super::*`: see `tests.rs`'s own doc
//! comment for why (the `gpui_kit::*`/`#[gpui_kit::test]` macro-budget issue
//! applies here too).
use super::{Harness, dock_panel_names, events_kind, harness, open_picker, open_targets};
use crate::config::saved_layouts::SavedLayout;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::{NavTarget, OpenMode};
use crate::ui::unrestored::UnrestoredPanel;
use crate::util::shell::test_support::press;
use crate::util::shell::{MainWindow, WindowMode};
use gpui_kit::{Entity, TestAppContext};

/// Like `tests.rs`'s own `fixture_layout`, but each of `extra_targets` names
/// its own context rather than always the window's first (`fixture_layout`'s
/// `open_target` always opens against the window's active context) - these
/// tests need a saved panel scoped to a context the *loading* window doesn't
/// hold, which a single-context fixture can't produce.
fn fixture_layout_scoped(
    cx: &mut TestAppContext,
    name: &str,
    contexts: Vec<String>,
    extra_targets: Vec<(NavTarget, String)>,
) -> SavedLayout {
    let window = cx.add_window(|window, cx| {
        let mut main = MainWindow::test_workspace(contexts.clone(), window, cx);
        for (target, context_name) in extra_targets {
            main.open_target_in(
                target,
                None,
                Some(context_name),
                Vec::new(),
                OpenMode::Foreground,
                window,
                cx,
            );
        }
        main
    });
    cx.run_until_parked();
    let dock = window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.read(cx).dump(cx)
        })
        .unwrap();
    SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: name.to_string(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts,
        dock,
        resource_panel_width: Some(333.0),
        window_width: 999.0,
        window_height: 555.0,
    }
}

/// The window's own held contexts, for this file's "never connects the
/// missing one" assertion - `WindowMode::Workspace`'s own field, reachable
/// here since a private field of a type defined in `util::shell::window` is
/// visible throughout `util::shell` and all of its descendants, which this
/// module is one of.
fn contexts(h: &mut Harness) -> Vec<String> {
    let main = h.main.clone();
    h.vcx.update(move |_window, cx| {
        let WindowMode::Workspace { contexts, .. } = &main.read(cx).mode else {
            panic!("a connected window is in workspace mode")
        };
        contexts.clone()
    })
}

/// The single `UnrestoredPanel` the dock built, panicking if there isn't
/// exactly one - every test here saves exactly one unrestorable panel
/// alongside Pods.
fn the_unrestored_panel(h: &mut Harness) -> Entity<UnrestoredPanel> {
    let main = h.main.clone();
    h.vcx.update(move |_window, cx| {
        let views = main.read(cx).test_dock_views(cx);
        let mut found = views
            .iter()
            .filter(|view| view.panel_name(cx) == crate::ui::unrestored::PANEL_NAME);
        let view = found.next().expect("exactly one placeholder panel");
        assert!(
            found.next().is_none(),
            "exactly one placeholder panel, not more"
        );
        Entity::from(view.as_ref())
    })
}

/// 5.1: Replace restores a saved panel scoped to a context this window
/// doesn't hold as a placeholder naming that context, in the same slot -
/// every other panel in the layout restores normally, and the window's held
/// contexts are unchanged (no auto-connect).
#[gpui_kit::test]
async fn loading_with_replace_restores_a_missing_context_panel_as_a_placeholder(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "other", ConnectionState::Connecting);
    });
    let fixture = fixture_layout_scoped(
        cx,
        "MissingContext",
        vec!["demo".into(), "other".into()],
        vec![(NavTarget::Kind(events_kind()), "other".to_string())],
    );
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods()],
        "the missing-context panel has no content key - only Pods restored with one"
    );
    assert_eq!(
        dock_panel_names(&mut h),
        vec!["Pods", crate::ui::unrestored::PANEL_NAME],
        "the Events panel restored as a placeholder instead of the real kind"
    );
    let placeholder = the_unrestored_panel(&mut h);
    h.vcx.update(|_window, cx| {
        let placeholder = placeholder.read(cx);
        assert_eq!(
            placeholder.panel_kind(),
            "Events",
            "names the kind it stands in for"
        );
        assert!(
            placeholder.reason().contains("other"),
            "{:?} names the missing context",
            placeholder.reason()
        );
    });
    assert_eq!(
        contexts(&mut h),
        vec!["demo".to_string()],
        "the window's held contexts are unchanged - no auto-connect"
    );
}

/// 5.1: Add restores a saved panel scoped to a context this window doesn't
/// hold as a placeholder added to the dock, rather than refusing it the way
/// `open_target_in` refuses any other request for a context the window
/// doesn't hold (which would build nothing at all) - every other panel in
/// the layout still opens normally, and the window's held contexts are
/// unchanged.
#[gpui_kit::test]
async fn loading_with_add_restores_a_missing_context_panel_as_a_placeholder(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "other", ConnectionState::Connecting);
    });
    let fixture = fixture_layout_scoped(
        cx,
        "MissingContextAdd",
        vec!["demo".into(), "other".into()],
        vec![(NavTarget::Kind(events_kind()), "other".to_string())],
    );
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "secondary-enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods()],
        "the placeholder has no content key - the window's own Pods is still the only keyed panel"
    );
    assert_eq!(
        dock_panel_names(&mut h),
        vec!["Pods", crate::ui::unrestored::PANEL_NAME],
        "the Events panel was added as a placeholder, alongside the window's own Pods"
    );
    let placeholder = the_unrestored_panel(&mut h);
    h.vcx.update(|_window, cx| {
        let placeholder = placeholder.read(cx);
        assert_eq!(placeholder.panel_kind(), "Events");
        assert!(
            placeholder.reason().contains("other"),
            "{:?} names the missing context",
            placeholder.reason()
        );
    });
    assert_eq!(
        contexts(&mut h),
        vec!["demo".to_string()],
        "Add never connects the missing context either"
    );
}
