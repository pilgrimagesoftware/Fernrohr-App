//! The Managed Fields tab (`resource-detail-ui-improvements` 5): one disclosure
//! row per field manager - manager, operation and when it last wrote -
//! collapsed by default and expanding to the fields it owns. Each row's toggle
//! is a tab stop, so Tab reaches it and Enter or Space flips it, as a click
//! does. Rows, not framed cards.

use super::model::ManagedFieldEntry;
use super::panel::PodDetailPanel;
use crate::k8s::resource::pods::format_age;
use crate::ui::typography::TypeRole as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Manager `index`'s disclosure toggle - its open-section key and element id.
/// Keyed by position, not manager name: two entries can share a manager (a
/// status subresource update versus the main resource), and one key would
/// toggle both.
pub(super) fn managed_field_key(index: usize) -> String {
    format!("mf-{index}")
}

/// The fields block shown while manager `index` is expanded.
pub(super) fn managed_field_body_selector(index: usize) -> String {
    format!("managed-fields-body-{index}")
}

/// One row's summary: `manager · operation · 5m ago`.
fn summary(entry: &ManagedFieldEntry, now: jiff::Timestamp) -> String {
    let mut parts = vec![entry.manager.clone(), entry.operation.clone()];
    if let Some(time) = entry.time {
        parts.push(format!(
            "{} ago",
            format_age(now.duration_since(time).as_secs())
        ));
    }
    parts.join(" · ")
}

impl PodDetailPanel {
    pub(super) fn render_managed_fields(
        &self,
        entries: &[ManagedFieldEntry],
        cx: &Context<Self>,
    ) -> AnyElement {
        let now = jiff::Timestamp::now();
        let space = crate::ui::space::spacing(cx);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(entries.iter().enumerate().map(|(index, entry)| {
                let key = managed_field_key(index);
                let open = self.open_sections.contains(&key);
                let this = cx.weak_entity();
                let toggle_key = key.clone();
                let toggle = Button::new(SharedString::from(key))
                    .icon(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .label(summary(entry, now))
                    .xsmall()
                    .ghost()
                    .frame_font(cx)
                    .on_click(move |_event, _window, cx| {
                        let _ = this.update(cx, |this: &mut Self, cx| {
                            if !this.open_sections.remove(&toggle_key) {
                                this.open_sections.insert(toggle_key.clone());
                            }
                            cx.notify();
                        });
                    });
                let body_selector = managed_field_body_selector(index);
                div()
                    .flex()
                    .flex_col()
                    .child(div().flex().child(toggle))
                    .when(open, |this| {
                        // Wraps rather than `whitespace_nowrap()` like the YAML
                        // view: the structured view scrolls vertically only, so
                        // an unwrapped deep ownership path would push the panel
                        // wider than it is.
                        this.child(
                            div()
                                .pl(space.panel_inset)
                                .code_font(cx)
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .debug_selector(move || body_selector.clone())
                                .child(entry.fields_json.clone()),
                        )
                    })
            }))
            .into_any_element()
    }
}
