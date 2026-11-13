//! Drawing the Values and Manifest tabs (helm-releases D6): which revision
//! they show, hidden until revealed with the reveal key shown, and once
//! revealed the text as plain monospace lines with Hide and Copy controls.

use super::commands::{CopyManifest, CopyValues, PANEL_KEY_CONTEXT, ToggleReveal};
use super::panel::{HelmReleasePanel, ReleaseTab};
use super::reveal::{RevealState, Revealed};
use crate::ui::style::{self, Tone};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

impl HelmReleasePanel {
    pub(super) fn text_tab(
        &self,
        tab: ReleaseTab,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let what = match tab {
            ReleaseTab::Values => "values",
            _ => "manifest",
        };
        let Some(version) = self.selected_revision(cx) else {
            return div().into_any_element();
        };
        let reveal_key = Kbd::binding_for_action(&ToggleReveal, Some(PANEL_KEY_CONTEXT), window);
        let this = cx.weak_entity();
        let toggle = move |label: &'static str| {
            let this = this.clone();
            Button::new(SharedString::from(format!("helm-release-{label}")))
                .label(label)
                .children(reveal_key.clone())
                .xsmall()
                .debug_selector(move || format!("helm-release-{}", label.to_lowercase()))
                .on_click(move |_, _window, cx| {
                    let _ = this.update(cx, |this, cx| this.toggle_reveal(cx));
                })
        };
        let heading = div()
            .text_color(cx.theme().muted_foreground)
            .debug_selector(|| "helm-release-text-revision".into())
            .child(format!("Revision {version}'s {what}"));
        let body = match &self.revealed {
            Some(Revealed {
                tab: shown,
                version: of,
                state,
            }) if *shown == tab && *of == version => match state {
                RevealState::Loading => div().child("\u{2026}").into_any_element(),
                RevealState::Failed(reason) => div()
                    .text_color(style::status(Tone::Bad, cx))
                    .child(format!(
                        "Couldn't read revision {version}'s {what}: {reason}"
                    ))
                    .into_any_element(),
                RevealState::Shown(_) => {
                    let text = self.revealed_text(tab, cx).unwrap_or_default();
                    let copy = Button::new("helm-release-copy")
                        .label(format!("Copy {}", tab.title()))
                        .xsmall()
                        .ghost()
                        .debug_selector(|| "helm-release-copy".into())
                        .on_click(move |_, window, cx| match tab {
                            ReleaseTab::Values => window.dispatch_action(Box::new(CopyValues), cx),
                            _ => window.dispatch_action(Box::new(CopyManifest), cx),
                        });
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().flex().gap_2().child(toggle("Hide")).child(copy))
                        .child(
                            super::render::selectable_lines("helm-release-text", text)
                                .debug_selector(|| "helm-release-text".into()),
                        )
                        .into_any_element()
                }
            },
            _ => div()
                .flex()
                .items_center()
                .gap_2()
                .debug_selector(|| "helm-release-hidden".into())
                .child(format!(
                    "The {what} are hidden: they can hold passwords and keys."
                ))
                .child(toggle("Reveal"))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading)
            .child(body)
            .into_any_element()
    }
}
