//! `resource-detail-ui-improvements` 2: the YAML view scrolls to every line and
//! folds each nested block - by click, by Tab and Space on its toggle, and
//! all at once with `z` / `shift-z`.

use super::fixtures::{kind, object, stub_panel, target};
use crate::command::CommandRegistry;
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::keymap::{self, KeymapConfig};
use crate::ui::yaml_view::{YamlLines, fold_toggle_id, line_selector};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase,
    VisualTestContext, WindowHandle, point, px,
};
use serde_json::json;

/// A ConfigMap far taller than the panel, with two managed-fields entries.
fn big_config_map() -> kube::api::DynamicObject {
    let data: serde_json::Map<String, serde_json::Value> = (0..200)
        .map(|n| (format!("key-{n:03}"), json!(format!("value {n}"))))
        .collect();
    object(json!({
        "apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {
            "name": "app-config", "namespace": "staging",
            "managedFields": [
                { "manager": "kubectl", "operation": "Update", "fieldsType": "FieldsV1",
                  "fieldsV1": { "f:data": { "f:key-000": {} } } },
                { "manager": "helm", "operation": "Update", "fieldsType": "FieldsV1",
                  "fieldsV1": { "f:data": { "f:key-001": {} } } },
            ],
        },
        "data": data,
    }))
}

/// A panel showing the ConfigMap's YAML, focused, with the real bindings.
fn yaml_panel(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<Root>,
    Entity<ObjectDetailPanel>,
    VisualTestContext,
) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let config_maps = kind("", "v1", "ConfigMap", true);
    let (window, panel) = stub_panel(
        cx,
        target(config_maps.clone(), Some("staging"), "app-config"),
        vec![config_maps],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |_, window, cx| {
            panel.update(cx, |panel, cx| panel.test_set_loaded(big_config_map(), cx));
            panel.read(cx).focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    vcx.simulate_keystrokes("y");
    vcx.run_until_parked();
    (window, panel, vcx)
}

/// The manifest's lines, as the view parses them.
fn lines(panel: &Entity<ObjectDetailPanel>, vcx: &mut VisualTestContext) -> YamlLines {
    let yaml = vcx.update(|_, cx| panel.read(cx).yaml().expect("loaded"));
    YamlLines::parse(&yaml)
}

/// The index of the first line whose text, trimmed, is `text`.
fn line_of(lines: &YamlLines, text: &str) -> usize {
    lines
        .lines
        .iter()
        .position(|line| line.text.trim() == text)
        .unwrap_or_else(|| panic!("no line {text:?}"))
}

fn drawn(window: WindowHandle<Root>, vcx: &mut VisualTestContext, line: usize) -> bool {
    let _ = vcx.update_window(window.into(), |_, window, cx| window.render_frame(cx));
    let selector: &'static str = line_selector(line).leak();
    vcx.debug_bounds(selector).is_some()
}

fn click_toggle(window: WindowHandle<Root>, vcx: &mut VisualTestContext, line: usize) {
    let center = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(fold_toggle_id(line))
                .expect("the fold toggle is drawn")
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(center, Modifiers::none());
    vcx.run_until_parked();
}

/// Spec: "A large manifest" - the view scrolls: a wheel over it moves its
/// offset, so every line can be brought into view.
#[gpui_kit::test]
async fn the_yaml_view_scrolls(cx: &mut TestAppContext) {
    let (window, panel, mut vcx) = yaml_panel(cx);
    let first = drawn(window, &mut vcx, 0);
    assert!(first, "the first line is drawn");
    let over = vcx.debug_bounds("yaml-line-0").expect("drawn").center();
    vcx.simulate_event(ScrollWheelEvent {
        position: over,
        delta: ScrollDelta::Pixels(point(px(0.), px(-600.))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    vcx.run_until_parked();
    let offset = vcx.update(|_, cx| panel.read(cx).yaml_view.scroll.offset());
    assert!(offset.y < px(0.), "the view scrolled down: {offset:?}");
}

/// Spec: "A large manifest" - folding `managedFields` shows it as one line until
/// unfolded, by click.
#[gpui_kit::test]
async fn managed_fields_fold_and_unfold_on_click(cx: &mut TestAppContext) {
    let (window, panel, mut vcx) = yaml_panel(cx);
    let lines = lines(&panel, &mut vcx);
    let managed = line_of(&lines, "managedFields:");
    let inside = managed + 1;
    assert!(drawn(window, &mut vcx, inside), "unfolded at first");

    click_toggle(window, &mut vcx, managed);
    assert!(
        drawn(window, &mut vcx, managed),
        "the block's own line stays"
    );
    assert!(
        !drawn(window, &mut vcx, inside),
        "its contents are folded away"
    );

    click_toggle(window, &mut vcx, managed);
    assert!(drawn(window, &mut vcx, inside), "and come back unfolded");
}

/// The keyboard: `z` folds everything to the top-level keys, `shift-z`
/// unfolds; and Tab reaches a toggle that Space flips.
#[gpui_kit::test]
async fn the_keyboard_folds_and_unfolds(cx: &mut TestAppContext) {
    let (window, panel, mut vcx) = yaml_panel(cx);
    let lines = lines(&panel, &mut vcx);
    let metadata = line_of(&lines, "metadata:");
    let name = line_of(&lines, "name: app-config");

    vcx.simulate_keystrokes("z");
    vcx.run_until_parked();
    assert!(drawn(window, &mut vcx, metadata));
    assert!(!drawn(window, &mut vcx, name), "z folded metadata");

    vcx.simulate_keystrokes("shift-z");
    vcx.run_until_parked();
    assert!(drawn(window, &mut vcx, name), "shift-z unfolded it");

    // Tab to the first toggle (metadata's - the first foldable line), Space.
    let first_toggle = lines.foldable().next().expect("something folds");
    assert_eq!(first_toggle, metadata);
    let mut focused = false;
    for _ in 0..10 {
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
        focused = vcx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window
                    .try_find(fold_toggle_id(metadata))
                    .and_then(|toggle| toggle.focused())
                    == Some(true)
            })
            .unwrap();
        if focused {
            break;
        }
    }
    assert!(focused, "Tab reaches the metadata toggle");
    // A key down *and* up, as a real press is: `Button` fires its keyboard
    // click on the release.
    let space = gpui_kit::Keystroke::parse("space").unwrap();
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    vcx.run_until_parked();
    assert!(!drawn(window, &mut vcx, name), "Space folded metadata");
}
