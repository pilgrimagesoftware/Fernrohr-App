//! #311 through a workspace window: a group picked in a panel's namespace
//! picker scopes that panel alone, as applying the set does, and survives a
//! saved layout - the restored panel has the group's namespaces, which the
//! picker names by the group again.

use super::*;
use crate::ui::namespace_picker::scope_label;
use crate::util::shell::panels::restored_panel_keys;

#[gpui_kit::test]
async fn a_group_picked_in_the_picker_scopes_the_panel_and_restores(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    // The Pods panel has focus: its picker, filtered to the group, which is
    // the first row after "All namespaces".
    press(&mut h.vcx, "n");
    type_text(&mut h, "workloads");
    press(&mut h.vcx, "down");
    press(&mut h.vcx, "enter");
    assert_eq!(
        scope_of(&mut h, "demo", &NavTarget::pods()),
        ["team-a", "team-b"]
    );
    assert!(
        scope_of(&mut h, "demo", &NavTarget::Kind(services())).is_empty(),
        "only the panel picked in moves"
    );

    let main = h.main.clone();
    let state = h.vcx.update(|_, cx| {
        let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
            panic!("a workspace window");
        };
        dock_area.read(cx).dump(cx)
    });
    let restored = restored_panel_keys(&state.center)
        .into_iter()
        .flatten()
        .find(|key| key.context_name == "demo" && key.target == NavTarget::pods())
        .expect("the Pods panel is saved");
    assert_eq!(restored.namespaces, ["team-a", "team-b"]);
    let label = h.vcx.update(|_, cx| scope_label(&restored.namespaces, cx));
    assert_eq!(
        label, "team-workloads",
        "the restored picker names the group"
    );
}
