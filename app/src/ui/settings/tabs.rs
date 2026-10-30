//! The Settings window's tab strip (#251): one tab per section across the top,
//! each with its icon, the shown one underlined. The tabs are buttons - Tab
//! reaches them, Enter or Space shows the section, as a click does - and Next
//! Tab and Previous Tab (`ctrl-tab`, `ctrl-shift-tab`) step through them from
//! anywhere in the window. A window too narrow for every tab scrolls the strip
//! sideways rather than letting it overflow.

use super::{KEY_CONTEXT, Section, SettingsWindow};
use crate::command::{Command, CommandRegistry};
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

actions!(settings, [NextTab, PreviousTab]);

/// The tab strip's element id and debug selector.
pub(crate) const TAB_BAR: &str = "settings-tabs";

pub(super) const NEXT_TAB_KEY: &str = "ctrl-tab";
pub(super) const PREVIOUS_TAB_KEY: &str = "ctrl-shift-tab";

impl Section {
    /// Every section, in tab order.
    pub(crate) const ALL: [Section; 5] = [
        Section::KeyboardShortcuts,
        Section::Appearance,
        Section::Panels,
        Section::Layouts,
        Section::AgentAccess,
    ];

    /// The icon on the section's tab.
    pub(super) fn icon(self) -> IconName {
        match self {
            Section::KeyboardShortcuts => IconName::Keyboard,
            Section::Appearance => IconName::Palette,
            Section::Panels => IconName::PanelsTopLeft,
            Section::Layouts => IconName::LayoutDashboard,
            Section::AgentAccess => IconName::Bot,
        }
    }

    /// The section `step` tabs on, wrapping at either end.
    pub(super) fn stepped(self, step: isize) -> Section {
        let count = Section::ALL.len() as isize;
        let at = Section::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or_default() as isize;
        Section::ALL[(at + step).rem_euclid(count) as usize]
    }
}

/// Next Tab and Previous Tab, while the window has focus.
pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "settings.next_tab",
        title: "Settings: Next Tab",
        default_binding: NEXT_TAB_KEY,
        context: Some(KEY_CONTEXT),
        action: Box::new(NextTab),
        menu: None,
    });
    registry.register(Command {
        id: "settings.previous_tab",
        title: "Settings: Previous Tab",
        default_binding: PREVIOUS_TAB_KEY,
        context: Some(KEY_CONTEXT),
        action: Box::new(PreviousTab),
        menu: None,
    });
}

impl SettingsWindow {
    /// The strip: every section's tab, scrolling sideways when it must.
    pub(super) fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let space = crate::ui::space::spacing(cx);
        div()
            .id(TAB_BAR)
            .debug_selector(|| TAB_BAR.into())
            .flex_none()
            .w_full()
            .flex()
            .items_end()
            .gap_1()
            .px(space.control_gap)
            .pt(space.control_gap)
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .overflow_x_scroll()
            .children(Section::ALL.map(|section| self.tab(section, cx)))
    }

    /// `section`'s tab: its icon and title, underlined while shown.
    ///
    /// The title is the button's own content rather than its `label`, so it
    /// keeps a debug selector of its own and never ellipsizes.
    fn tab(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        use gpui_kit::component::Selectable as _;
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        use gpui_kit::prelude::FluentBuilder as _;
        let shown = self.section == section;
        let label = section.label_selector();
        let underline = if shown {
            crate::ui::style::accent(cx)
        } else {
            gpui_kit::transparent_black()
        };
        div()
            .flex_none()
            .pb_1()
            .border_b_2()
            .border_color(underline)
            .child(
                Button::new(section.button_id())
                    .accessibility_label(section.title())
                    .icon(section.icon())
                    .child(
                        div()
                            .debug_selector(move || label.into())
                            .whitespace_nowrap()
                            .child(section.title()),
                    )
                    .ghost()
                    .when(shown, |button| button.selected(true))
                    .on_click(
                        cx.listener(move |this, _event, window, cx| this.show(section, window, cx)),
                    ),
            )
    }
}
