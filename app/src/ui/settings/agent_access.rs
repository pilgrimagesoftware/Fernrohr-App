//! The Settings window's Agent access section (`agent-mcp`: In-app agent
//! setup instructions): what an agent can do through Fernrohr, what it needs,
//! and the command registering Fernrohr with each harness, each beside a copy
//! button. OpenCode's entry also offers its config snippet.
//!
//! The copy buttons are icon buttons with tooltips naming what they copy
//! (`icon-buttons.md`): tab stops that Enter or Space presses, as a click
//! does. Copy MCP Setup Command (`ui::agent_setup`) reaches the same commands
//! from the palette. Instead of commands, the section warns when the
//! executable won't stay where it is, and says when this platform has no
//! endpoint.

use crate::mcp::setup::{AgentSetup, HARNESSES, SNIPPET_HARNESS, SNIPPET_NOTE, opencode_snippet};
use crate::ui::copy::{CopiedFeedback, copy_with_feedback};
use crate::ui::typography::TypeRole as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::*;
use std::path::Path;

/// The debug selectors of the section's states.
pub(crate) const COMMANDS_SELECTOR: &str = "agent-access-commands";
pub(crate) const UNSTABLE_SELECTOR: &str = "agent-access-unstable";
pub(crate) const UNAVAILABLE_SELECTOR: &str = "agent-access-unavailable";

/// The background MCP checkbox (quiet mode, #236), for tests.
pub(crate) const BACKGROUND_CHECKBOX: &str = "agent-access-background-mcp";

/// What the checkbox's note says it changes.
const BACKGROUND_NOTE: &str = "When the last window closes, Fernrohr keeps running in the menu \
     bar. With this on, agents can still reach it; with it off, the MCP server stops then and \
     starts again with the next window.";

/// The snippet's copy button's tooltip.
pub(crate) const SNIPPET_TOOLTIP: &str = "Copy OpenCode config snippet";

/// The element id of `harness`'s copy button.
pub(crate) fn copy_id(harness_id: &str) -> String {
    format!("agent-access-copy-{harness_id}")
}

/// The element id of OpenCode's snippet copy button.
pub(crate) const SNIPPET_COPY_ID: &str = "agent-access-copy-opencode-snippet";

/// What the section says before anything else, whatever this platform offers.
const ABOUT: &str = "Agents that speak MCP can read the clusters you have connected and open \
     panels here, bringing this window to the front. They can connect another cluster or run \
     a few everyday actions, each one only once you approve it in this window. Fernrohr must \
     be running for an agent to reach it.";

/// The section: what agents can do, then how to add Fernrohr to one - each
/// its own card (#251).
pub(super) fn section(window: &mut Window, cx: &mut App) -> impl IntoElement {
    use super::card::{body, card};
    let muted = cx.theme().muted_foreground;
    let setup = match crate::ui::agent_setup::setup(cx) {
        AgentSetup::Ready { exe } => commands(&exe, window, cx).into_any_element(),
        AgentSetup::Unstable { why, .. } => {
            let warning = cx.theme().warning;
            let notice = div()
                .debug_selector(|| UNSTABLE_SELECTOR.into())
                .flex()
                .items_start()
                .gap_2()
                .text_color(warning)
                .child(Icon::new(IconName::TriangleAlert).small())
                .child(div().flex_1().min_w_0().child(why.explanation()));
            card("Setup", cx).child(notice).into_any_element()
        }
        AgentSetup::Unavailable => card("Setup", cx)
            .child(
                div()
                    .debug_selector(|| UNAVAILABLE_SELECTOR.into())
                    .text_color(muted)
                    .child("Agent access isn't available on this platform: its MCP endpoint needs a Unix-domain socket."),
            )
            .into_any_element(),
    };
    body(cx)
        .child(card("About", cx).child(div().text_sm().child(ABOUT)))
        .children(crate::util::shell::QUIET_MODE.then(|| background(cx)))
        .child(setup)
}

/// The background MCP setting, in its own card: a checkbox dispatching the
/// same commands the palette offers, and what it changes. Only where quiet
/// mode exists (macOS): elsewhere the server stops with the app, when the
/// last window closes.
fn background(cx: &App) -> impl IntoElement + use<> {
    use super::card::{card, hint};
    let keep =
        crate::ui::background_mcp::current(cx) == crate::config::ui::BackgroundMcp::KeepRunning;
    card("Background", cx)
        .child(
            gpui_kit::component::checkbox::Checkbox::new(BACKGROUND_CHECKBOX)
                .label(crate::ui::background_mcp::LABEL)
                .checked(keep)
                .on_click(|checked, window, cx| {
                    let action: Box<dyn Action> = if *checked {
                        Box::new(crate::ui::background_mcp::KeepMcpInBackground)
                    } else {
                        Box::new(crate::ui::background_mcp::StopMcpWithLastWindow)
                    };
                    window.dispatch_action(action, cx);
                }),
        )
        .child(hint(BACKGROUND_NOTE, cx))
}

/// A card with one row per harness - its name, its command, and the copy
/// button - then OpenCode's config snippet in a card of its own.
fn commands(exe: &Path, window: &mut Window, cx: &mut App) -> impl IntoElement {
    use super::card::{card, hint};
    let space = crate::ui::space::spacing(cx);
    let rows: Vec<AnyElement> = HARNESSES
        .iter()
        .map(|harness| {
            row(
                harness.name,
                harness.command(exe),
                copy_id(harness.id),
                harness.copy_tooltip,
                window,
                cx,
            )
        })
        .collect();
    let snippet = HARNESSES
        .iter()
        .any(|harness| harness.id == SNIPPET_HARNESS)
        .then(|| {
            card("OpenCode config file", cx)
                .child(hint(SNIPPET_NOTE, cx))
                .child(row(
                    "",
                    opencode_snippet(exe),
                    SNIPPET_COPY_ID.to_string(),
                    SNIPPET_TOOLTIP,
                    window,
                    cx,
                ))
        });
    div()
        .debug_selector(|| COMMANDS_SELECTOR.into())
        .flex()
        .flex_col()
        .gap(space.section_gap)
        .child(
            card("Add Fernrohr to your agent", cx)
                .child(hint(
                    "Run one of these once: it adds Fernrohr for every project.",
                    cx,
                ))
                .children(rows),
        )
        .children(snippet)
}

/// A label, `text` in the code face, and a copy button `id` copying it.
fn row(
    label: &'static str,
    text: String,
    id: String,
    tooltip: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let element_id = ElementId::Name(id.clone().into());
    let feedback = CopiedFeedback::of(&element_id, window, cx);
    let copied = feedback.read(cx).shown();
    let value = text.clone();
    let button = Button::new(element_id)
        .icon(if copied {
            IconName::Check
        } else {
            IconName::Copy
        })
        .xsmall()
        .ghost()
        .on_click(move |_event, _window, cx| copy_with_feedback(&value, &feedback, cx));
    div()
        .flex()
        .items_start()
        .gap_3()
        .child(div().w(rems(6.5)).flex_none().child(label))
        .child(div().flex_1().min_w_0().code_font(cx).text_sm().child(text))
        .child(crate::ui::icon_tooltip::with_tooltip(
            format!("{id}-tooltip"),
            tooltip,
            button,
        ))
        .into_any_element()
}
