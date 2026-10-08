//! Section 5.3: an unrecognised panel kind in a saved layout's dock JSON
//! doesn't fail the whole load - every other panel still restores, and that
//! slot becomes a placeholder. This needs no new code: `DockArea::load`'s own
//! registry fallback already does it for the automatic restore, and Replace
//! rebuilds through the identical path (`saved_layouts::load::load_replace`).
//!
//! That fallback is gpui-component's own `InvalidPanel` ("unknown panel"),
//! not `ui::unrestored`'s - the latter only ever stands in for a *registered*
//! kind whose saved *data* didn't parse (`restore_with`'s own job, used by
//! every `register_restore` in `k8s::resource::*`). An entirely unregistered
//! `panel_name` never reaches a kind's restore function at all, so there is
//! nothing for `restore_with` to wrap; `PanelRegistry::build_panel` itself
//! returns `None` and `DockSkin::build_placeholder` substitutes
//! `InvalidPanel` directly. design.md's Context section reads as though the
//! two were the same mechanism ("This is the same mechanism the automatic
//! restore already falls back to for an unknown panel kind") - this task's
//! final report flags that as a documentation mismatch against the vendored
//! `gpui-component`/`gpui-base` behavior, not a bug in this change.
use super::{dock_panel_names, harness, open_picker, open_targets};
use crate::config::saved_layouts::SavedLayout;
use crate::ui::nav::NavTarget;
use crate::util::shell::test_support::press;
use gpui_kit::TestAppContext;
use gpui_kit::component::dock::{DockAreaState, PanelInfo, PanelState};

/// A two-leaf dock, nested the same way a real one dumps (one split holding
/// one tab group) - mirrors `ui::unrestored::tests`' own `DockHost` fixture
/// shape, the smallest tree `DockArea::load` accepts.
fn hand_built_dock(leaves: Vec<PanelState>) -> DockAreaState {
    DockAreaState {
        center: PanelState {
            panel_name: "StackPanel".to_string(),
            children: vec![PanelState {
                panel_name: "TabPanel".to_string(),
                children: leaves,
                info: PanelInfo::Tabs { active_index: 0 },
            }],
            info: PanelInfo::Stack {
                sizes: Vec::new(),
                axis: 0,
            },
        },
        ..Default::default()
    }
}

#[gpui_kit::test]
async fn loading_with_replace_restores_an_unrecognized_panel_kind_as_a_placeholder_too(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx);
    let leaves = vec![
        PanelState {
            panel_name: "Pods".to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": "demo",
                "namespaces": [],
            })),
        },
        PanelState {
            panel_name: "FutureWidget".to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({ "context_name": "demo" })),
        },
    ];
    let fixture = SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: "UnknownKind".to_string(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["demo".to_string()],
        dock: hand_built_dock(leaves),
        resource_panel_width: Some(240.0),
        window_width: 800.0,
        window_height: 600.0,
    };
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods()],
        "the unrecognised panel has no content key - only Pods restored with one"
    );
    assert_eq!(
        dock_panel_names(&mut h),
        vec!["Pods", "InvalidPanel"],
        "both slots restored as something: Pods for real, the unknown kind as \
         the dock's own unknown-panel placeholder"
    );
}
