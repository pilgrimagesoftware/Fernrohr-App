//! A typed label panel's context (`selector-logs-context-picker`): the control
//! in its bar naming the context it searches, and the keyboard's picker, both
//! shown only while its window holds more than one context. Neither switches
//! anything itself. Each asks the window by dispatching
//! [`SwitchLabelLogsContext`], and `MainWindow` replaces the panel with one in
//! the context asked for (D1), or focuses that context's own typed panel (D2).

use super::*;
use crate::util::logs::commands::{PICK_CONTEXT_KEY, SwitchLabelLogsContext};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;

/// The context control's button, for tests.
pub(in crate::util::logs) const CONTEXT_CONTROL: &str = "logs-label-context";

impl LogsPanel {
    /// The contexts a typed panel can switch between - its window's, in order -
    /// or `None` for any other panel, or one whose window holds just one.
    fn context_choices(&self, window: &mut Window, cx: &App) -> Option<Vec<String>> {
        if !self.is_typed_labels() {
            return None;
        }
        let contexts = crate::util::shell::window_contexts(window, cx);
        (contexts.len() > 1).then_some(contexts)
    }

    /// The control naming the context a typed panel searches: its key, then
    /// a button reading the context that drops down the window's contexts,
    /// the current one checked. `None` while there's nothing to choose.
    pub(in crate::util::logs) fn context_control(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let contexts = self.context_choices(window, cx)?;
        let current = self.scope.context_name.clone();
        let key = Kbd::binding_for_action(&PickLogsContext, Some(PANEL_KEY_CONTEXT), window)
            .unwrap_or_else(|| {
                Kbd::new(Keystroke::parse(PICK_CONTEXT_KEY).expect("valid keybinding"))
            });
        let this = cx.weak_entity();
        Some(
            div()
                .flex()
                .items_center()
                .gap_1()
                .flex_shrink_0()
                .child(key)
                .child(
                    Button::new(CONTEXT_CONTROL)
                        .label(current.clone())
                        .icon(gpui_kit::assets::IconName::ChevronDown)
                        .xsmall()
                        .ghost()
                        .tab_stop(false)
                        .tooltip("Context the selector searches")
                        .dropdown_menu(move |menu, _window, _cx| {
                            contexts.iter().fold(menu, |menu, context| {
                                let (this, picked) = (this.clone(), context.clone());
                                menu.item(
                                    gpui_kit::component::menu::PopupMenuItem::new(context.clone())
                                        .checked(*context == current)
                                        .on_click(move |_event, window, cx| {
                                            let _ = this.update(cx, |this: &mut Self, cx| {
                                                this.request_context(picked.clone(), window, cx);
                                            });
                                        }),
                                )
                            })
                        }),
                )
                .into_any_element(),
        )
    }

    /// Asks the window to switch this panel to `context` - nothing for the
    /// context it already searches.
    fn request_context(&mut self, context: String, window: &mut Window, cx: &mut Context<Self>) {
        if context != self.scope.context_name {
            let action = SwitchLabelLogsContext {
                panel: cx.entity_id(),
                context,
            };
            window.dispatch_action(Box::new(action), cx);
        }
    }

    /// `PickLogsContext`: opens the keyboard's picker over the window's
    /// contexts. Nothing in a panel with no choice to offer.
    pub(in crate::util::logs) fn open_context_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(contexts) = self.context_choices(window, cx) else {
            return;
        };
        // `WindowExt`'s dialog calls expect a `Root`.
        if !matches!(window.root::<Root>(), Some(Some(_))) {
            return;
        }
        let current = self.scope.context_name.clone();
        let panel = cx.entity();
        let return_focus = self.focus_handle.clone();
        let picker =
            cx.new(|cx| ContextPicker::new(contexts, current, panel, return_focus, window, cx));
        let state = picker.read(cx).state.clone();
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let picker = picker.clone();
            dialog
                .title("Pick Context")
                .content(move |content, _window, _cx| content.child(picker.clone()))
        });
        state.update(cx, |state, cx| state.focus(window, cx));
    }

    /// What a replacement panel carries over (D1): the selector's text and the
    /// container filter.
    pub(crate) fn carried(&self) -> (Option<String>, Option<String>) {
        let Some(following) = &self.labels else {
            return (None, None);
        };
        (
            following.selector.as_ref().map(|(text, _)| text.clone()),
            following.container.clone(),
        )
    }

    /// Takes on what [`Self::carried`] read off the panel this one replaces.
    pub(crate) fn carry_over(
        &mut self,
        selector: Option<String>,
        container: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.set_container_filter(container, cx);
        if let Some(selector) = selector {
            self.apply_selector_text(&selector, cx);
        }
    }
}

/// The keyboard's context picker: the window's contexts, filterable, with
/// one selection that the arrows move and hover never does (the
/// `links::go_to` rule).
struct ContextPicker {
    contexts: Vec<String>,
    current: String,
    panel: Entity<LogsPanel>,
    return_focus: FocusHandle,
    state: Entity<CommandState>,
    selected: Option<usize>,
}

impl ContextPicker {
    fn new(
        contexts: Vec<String>,
        current: String,
        panel: Entity<LogsPanel>,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            contexts,
            current,
            panel,
            return_focus,
            state: cx.new(|cx| CommandState::new(window, cx)),
            selected: None,
        }
    }

    /// Picks context `index`: closes the dialog, puts focus back on the panel,
    /// then asks for the switch from there.
    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(context) = self.contexts.get(index).cloned() else {
            return;
        };
        if matches!(window.root::<Root>(), Some(Some(_))) {
            window.close_dialog(cx);
        }
        self.return_focus.focus(window, cx);
        let panel = self.panel.clone();
        window.defer(cx, move |window, cx| {
            panel.update(cx, |panel, cx| panel.request_context(context, window, cx));
        });
    }
}

impl Render for ContextPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let items: Vec<CommandItem> = self
            .contexts
            .iter()
            .map(|context| {
                let label = if *context == self.current {
                    format!("{context} (current)")
                } else {
                    context.clone()
                };
                CommandItem::new().label(label)
            })
            .collect();
        CommandList::new(&self.state)
            .items(items)
            .placeholder("Context…")
            .on_select({
                let this = this.clone();
                move |index_path, window, cx| {
                    if !window.last_input_was_keyboard() {
                        return;
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.selected = Some(index_path.row);
                        cx.notify();
                    });
                }
            })
            // Enter picks the selection; a click picks the row clicked.
            .on_confirm(move |index_path, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let index = if window.last_input_was_keyboard() {
                        this.selected.unwrap_or(index_path.row)
                    } else {
                        index_path.row
                    };
                    this.pick(index, window, cx);
                });
            })
    }
}
