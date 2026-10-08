//! `list-search` #189's regression for the events browser, now that its search
//! box runs through the shared `crate::ui::list_search::ListSearch`: searching
//! stays case-insensitive, and the hint row's key is the live keymap's, not
//! just its fallback.

use super::super::commands::{FocusSearch, PANEL_KEY_CONTEXT};
use super::*;

/// Design D3's search was always substring, case-sensitive or not depending
/// on the query - pin that the shared matcher kept it case-insensitive.
#[gpui_kit::test]
async fn searching_is_case_insensitive(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    focus_table(&mut h);
    press(&mut h.vcx, "/");
    h.vcx.simulate_input("IMAGEPULLBACKOFF");
    h.vcx.run_until_parked();
    assert_eq!(shown(&mut h), ["e3"], "matched despite the different case");
}

/// A `keymap.toml` override of `events.search` rebinds `/`: the old default no
/// longer focuses the box, the new key does, and the hint row - which reads
/// the same live keymap through `Kbd::binding_for_action` - has a key to show.
#[gpui_kit::test]
async fn the_search_hint_follows_a_keymap_override(cx: &mut TestAppContext) {
    let mut h = harness_overriding_keymap(cx, fixture(), &[("events.search", "ctrl-/")], |_| {});
    focus_table(&mut h);

    press(&mut h.vcx, "/");
    let focused_by_default_key = h
        .vcx
        .update(|window, cx| h.panel.read(cx).search.is_focused(window, cx));
    assert!(
        !focused_by_default_key,
        "the default key no longer focuses the search box once rebound"
    );

    press(&mut h.vcx, "ctrl-/");
    let focused_by_override = h
        .vcx
        .update(|window, cx| h.panel.read(cx).search.is_focused(window, cx));
    assert!(focused_by_override, "the rebound key does");

    let hinted = h
        .vcx
        .update(|window, _| Kbd::binding_for_action(&FocusSearch, Some(PANEL_KEY_CONTEXT), window));
    assert!(hinted.is_some(), "the hint row has a live key to show");
}
