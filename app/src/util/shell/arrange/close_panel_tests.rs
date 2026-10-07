//! Fernrohr#129: closing one panel - `Cmd-W`, or its tab's close control -
//! asks first when that would discard an unsaved edit, through the same
//! confirmation Close Group uses, and irreversibly: Enter and Escape keep the
//! panel; Tab to Close Panel and Enter closes it.

use super::tests::{Harness, edit_a_deployment, harness};
use crate::ui::nav::OpenedPanel;
use crate::util::shell::WindowMode;
use gpui_kit::component::dock::PanelId;
use gpui_kit::{Modifiers, MouseButton, TestAppContext};

impl Harness {
    /// The Deployment panel holding the unsaved edit - `edit_a_deployment`
    /// edits a split copy, beside the one it opened - while the dock has it.
    fn deployment_panel(&mut self) -> Option<PanelId> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace {
                open_panels,
                dock_area,
                ..
            } = &main.read(cx).mode
            else {
                return None;
            };
            open_panels.iter().find_map(|open| match &open.panel {
                Some(OpenedPanel::ObjectDetail(panel))
                    if panel.read(cx).close_warning().is_some()
                        && dock_area.read(cx).panel(open.id).is_some() =>
                {
                    Some(open.id)
                }
                _ => None,
            })
        })
    }

    fn click_close_of(&mut self, panel: PanelId) {
        let selector: &'static str = format!("panel-close-{}", panel.as_u64()).leak();
        let bounds = self
            .vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("`{selector}` is drawn"));
        self.vcx
            .simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::none());
        self.vcx
            .simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::none());
        self.vcx.run_until_parked();
    }
}

/// `Cmd-W` - the menu's platform key, `secondary-w` - on a panel with an
/// unsaved edit.
#[gpui_kit::test]
async fn cmd_w_on_an_unsaved_edit_asks_before_closing(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    edit_a_deployment(&mut h);
    let panel = h.deployment_panel().expect("the edited panel");

    h.press("secondary-w");
    assert!(h.dialog_open(), "an unsaved edit: it asks");
    h.first_frame();
    h.press("enter");
    assert!(!h.dialog_open(), "Enter answers the question");
    assert_eq!(
        h.deployment_panel(),
        Some(panel),
        "and, irreversible, keeps the panel"
    );

    h.press("secondary-w");
    h.first_frame();
    h.press("escape");
    assert_eq!(h.deployment_panel(), Some(panel), "Escape keeps it too");

    h.press("secondary-w");
    h.first_frame();
    h.press("tab enter");
    assert_eq!(
        h.deployment_panel(),
        None,
        "Tab to Close Panel, Enter: closed"
    );
}

/// The tab's close control asks the same question.
#[gpui_kit::test]
async fn the_close_control_on_an_unsaved_edit_asks_before_closing(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    edit_a_deployment(&mut h);
    let panel = h.deployment_panel().expect("the edited panel");

    h.click_close_of(panel);
    assert!(h.dialog_open(), "an unsaved edit: it asks");
    h.first_frame();
    h.press("escape");
    assert_eq!(h.deployment_panel(), Some(panel), "Escape keeps the panel");

    h.click_close_of(panel);
    h.first_frame();
    h.press("tab enter");
    assert_eq!(h.deployment_panel(), None, "confirmed: closed");
}
