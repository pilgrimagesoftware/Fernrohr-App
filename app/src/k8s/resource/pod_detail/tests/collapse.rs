//! Large ConfigMap values in the Configuration tab start collapsed
//! (`pod-configuration-tab` 5.1-5.2): a long or multi-line value shows its
//! preview and an expand control, a short one shows whole with none, one
//! value expands by keyboard or mouse while the rest stay collapsed, and
//! showing the tab again collapses them all. Secret values never collapse: a
//! Secret key has no collapse control, hidden or revealed.
//!
//! Card 0 is Secret `db` (`password`, `username`), card 1 ConfigMap `app-env`
//! (`BANNER`, `LOG_LEVEL`, `MOTD`, `config.yaml`, in key order).

use super::config_fixture::{
    BANNER, CONFIG_YAML, Harness, MOTD, PASSWORD, focus_panel, harness, open_configuration,
    press_by_keyboard, reveal_password_by_keyboard, shown, wait_for,
};
use crate::k8s::resource::pod_detail::configuration::{expand_button_id, key_label_id, value_id};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, ElementId, Modifiers, Pixels, TestAppContext, VisualTestContext, px,
};

const SECRET: usize = 0;
const CONFIG_MAP: usize = 1;
const BANNER_KEY: usize = 0;
const LOG_LEVEL_KEY: usize = 1;
const MOTD_KEY: usize = 2;
const CONFIG_YAML_KEY: usize = 3;

/// What value `index` of card `card` draws, read after a fresh frame.
fn drawn(vcx: &mut VisualTestContext, h: &Harness, card: usize, index: usize) -> Option<String> {
    vcx.update_window(h.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(value_id(card, index))
            .and_then(|value| value.value().map(str::to_string))
    })
    .unwrap()
}

fn has_control(vcx: &mut VisualTestContext, h: &Harness, id: ElementId) -> bool {
    vcx.update_window(h.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .unwrap()
}

fn bounds(vcx: &mut VisualTestContext, h: &Harness, id: ElementId) -> Bounds<Pixels> {
    vcx.update_window(h.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id.clone())
            .unwrap_or_else(|| panic!("{id:?} is drawn"))
            .bounds()
    })
    .unwrap()
}

fn click(vcx: &mut VisualTestContext, h: &Harness, id: ElementId) {
    let center = vcx
        .update_window(h.window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(id.clone())
                .unwrap_or_else(|| panic!("{id:?} is drawn"))
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(center, Modifiers::none());
    vcx.run_until_parked();
}

#[gpui_kit::test]
async fn large_values_start_collapsed_and_expand_one_at_a_time(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    open_configuration(&mut vcx, &h);

    // A long value and a multi-line one show only their previews.
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, BANNER_KEY).as_deref(),
        Some("Welcome to the stagi…")
    );
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, CONFIG_YAML_KEY).as_deref(),
        Some("server:…")
    );
    // A short value shows whole, with no control.
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, LOG_LEVEL_KEY).as_deref(),
        Some("debug")
    );
    assert!(!has_control(
        &mut vcx,
        &h,
        expand_button_id(CONFIG_MAP, LOG_LEVEL_KEY)
    ));
    // So does one of exactly 100 characters: only past 100 is large.
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, MOTD_KEY).as_deref(),
        Some(MOTD)
    );
    assert!(!has_control(
        &mut vcx,
        &h,
        expand_button_id(CONFIG_MAP, MOTD_KEY)
    ));

    // Tab + Space expands that one value; the other stays collapsed.
    press_by_keyboard(&mut vcx, &h, expand_button_id(CONFIG_MAP, CONFIG_YAML_KEY));
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, CONFIG_YAML_KEY).as_deref(),
        Some(CONFIG_YAML)
    );
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, BANNER_KEY).as_deref(),
        Some("Welcome to the stagi…")
    );

    // A click expands the other, and collapses it again.
    click(&mut vcx, &h, expand_button_id(CONFIG_MAP, BANNER_KEY));
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, BANNER_KEY).as_deref(),
        Some(BANNER)
    );
    click(&mut vcx, &h, expand_button_id(CONFIG_MAP, BANNER_KEY));
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, BANNER_KEY).as_deref(),
        Some("Welcome to the stagi…")
    );
}

#[gpui_kit::test]
async fn showing_the_tab_again_collapses_every_value(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    open_configuration(&mut vcx, &h);

    press_by_keyboard(&mut vcx, &h, expand_button_id(CONFIG_MAP, CONFIG_YAML_KEY));
    click(&mut vcx, &h, expand_button_id(CONFIG_MAP, BANNER_KEY));
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, CONFIG_YAML_KEY).as_deref(),
        Some(CONFIG_YAML)
    );
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, BANNER_KEY).as_deref(),
        Some(BANNER)
    );

    focus_panel(&mut vcx, &h);
    vcx.simulate_keystrokes("1");
    vcx.simulate_keystrokes("3");
    vcx.run_until_parked();
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, CONFIG_YAML_KEY).as_deref(),
        Some("server:…")
    );
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, BANNER_KEY).as_deref(),
        Some("Welcome to the stagi…")
    );
}

/// A Secret value is hidden until revealed and shown in full once it is - so
/// even a long one has no collapse control, hidden or revealed.
#[gpui_kit::test]
async fn a_secret_key_has_no_collapse_control(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    let panel = h.panel.clone();
    open_configuration(&mut vcx, &h);
    assert!(
        PASSWORD.chars().count() > 100,
        "the fixture's value is large"
    );

    assert!(!has_control(&mut vcx, &h, expand_button_id(SECRET, 0)));
    reveal_password_by_keyboard(&mut vcx, &h);
    wait_for(&mut vcx, &panel, shown);
    assert!(
        has_control(&mut vcx, &h, value_id(SECRET, 0)),
        "it's revealed"
    );
    assert!(!has_control(&mut vcx, &h, expand_button_id(SECRET, 0)));
}

/// The expand control sits right after its key's label - on the key's line,
/// a gap past the label's end - rather than at the far edge of a wide row,
/// and it stays put when the value expands.
#[gpui_kit::test]
async fn the_expand_control_sits_beside_its_key(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    open_configuration(&mut vcx, &h);

    let key = bounds(&mut vcx, &h, key_label_id(CONFIG_MAP, CONFIG_YAML_KEY));
    let value = bounds(&mut vcx, &h, value_id(CONFIG_MAP, CONFIG_YAML_KEY));
    let control = bounds(&mut vcx, &h, expand_button_id(CONFIG_MAP, CONFIG_YAML_KEY));
    assert!(
        value.size.width > px(400.),
        "the row is wide enough to tell beside from right-aligned: {value:?}"
    );
    let gap = control.left() - key.right();
    assert!(
        gap >= px(0.) && gap < px(16.),
        "the control follows the key's label: key {key:?}, control {control:?}"
    );
    assert!(
        control.top() < key.bottom() && key.top() < control.bottom(),
        "the control is on the key's line: key {key:?}, control {control:?}"
    );
    assert!(
        control.bottom() <= value.top(),
        "the control is above the value, not beside it: {control:?}, {value:?}"
    );

    press_by_keyboard(&mut vcx, &h, expand_button_id(CONFIG_MAP, CONFIG_YAML_KEY));
    assert_eq!(
        drawn(&mut vcx, &h, CONFIG_MAP, CONFIG_YAML_KEY).as_deref(),
        Some(CONFIG_YAML)
    );
    assert_eq!(
        bounds(&mut vcx, &h, expand_button_id(CONFIG_MAP, CONFIG_YAML_KEY)).origin,
        control.origin,
        "expanding doesn't move the control"
    );
}
