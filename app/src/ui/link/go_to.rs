//! The keyboard route to a detail view's links: `g` opens a "Go to…" picker
//! listing every followable reference the view shows. `resource-links`
//! section 4.
//!
//! One command, `links.go_to`, serves every detail view: each adds
//! [`LINKS_KEY_CONTEXT`] to its own key context, so the binding fires only
//! while one of them is focused, and the view hands the picker the same
//! references it renders - so the picker and the visible links can't disagree.

use super::FollowReference;
use crate::command::{Command, CommandRegistry};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::viewer::viewer_for;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

actions!(links, [GoToReference]);

/// The key-context identifier every detail view with links adds to its own.
pub const LINKS_KEY_CONTEXT: &str = "ReferenceLinks";
pub const GO_TO_COMMAND_ID: &str = "links.go_to";
pub const GO_TO_DEFAULT_BINDING: &str = "g";

/// `links.go_to`: a palette entry while a detail view is focused, a
/// `keymap.toml` override by id, and no menu slot - it acts on one panel.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: GO_TO_COMMAND_ID,
        title: "Go to Reference…",
        default_binding: GO_TO_DEFAULT_BINDING,
        context: Some(LINKS_KEY_CONTEXT),
        action: Box::new(GoToReference),
        menu: None,
    });
}

/// The `go_to` key as currently bound, for a view's hint bar.
pub fn go_to_key(window: &Window) -> Kbd {
    Kbd::binding_for_action(&GoToReference, Some(LINKS_KEY_CONTEXT), window).unwrap_or_else(|| {
        Kbd::new(Keystroke::parse(GO_TO_DEFAULT_BINDING).expect("valid keybinding"))
    })
}

/// One reference a view shows, and the field it came from ("Node", "Volume
/// config", ...) - what tells two references to one Secret apart in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoToEntry {
    pub target: ObjectRef,
    pub field: String,
}

impl GoToEntry {
    pub fn new(target: ObjectRef, field: impl Into<String>) -> Self {
        Self {
            target,
            field: field.into(),
        }
    }

    fn label(&self) -> String {
        format!("{} {} · {}", self.target.kind, self.target.name, self.field)
    }
}

/// The entries the picker can follow: those `viewer_for` resolves against
/// `kinds`, each object once (its first field wins).
pub fn followable(
    entries: impl IntoIterator<Item = GoToEntry>,
    kinds: Option<&[DiscoveredKind]>,
) -> Vec<GoToEntry> {
    let mut followable: Vec<GoToEntry> = Vec::new();
    for entry in entries {
        if viewer_for(&entry.target, kinds).is_some()
            && !followable.iter().any(|seen| seen.target == entry.target)
        {
            followable.push(entry);
        }
    }
    followable
}

/// Opens the picker over `entries` as a dialog. Following one refocuses
/// `return_focus` (the detail view) first, so `FollowReference` is dispatched
/// from inside the window's workspace rather than from the dialog layer, which
/// sits outside it. Escape closes the dialog and the window's root returns
/// focus to the same view.
pub fn open(
    entries: Vec<GoToEntry>,
    context_name: String,
    return_focus: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    // `WindowExt`'s dialog calls expect a `Root`; a window without one has
    // nowhere to show the dialog.
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let picker = cx.new(|cx| GoToPicker::new(entries, context_name, return_focus, window, cx));
    let state = picker.read(cx).state.clone();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let picker = picker.clone();
        dialog
            .title("Go to")
            .content(move |content, _window, _cx| content.child(picker.clone()))
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

/// The picker's own state: the entries, and the one selection clicks and the
/// keyboard move. `Command` also keeps a highlight that follows the pointer;
/// that highlight is never this selection.
struct GoToPicker {
    entries: Vec<GoToEntry>,
    context_name: String,
    return_focus: FocusHandle,
    state: Entity<CommandState>,
    selected: Option<usize>,
}

impl GoToPicker {
    fn new(
        entries: Vec<GoToEntry>,
        context_name: String,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected = (!entries.is_empty()).then_some(0);
        Self {
            entries,
            context_name,
            return_focus,
            state: cx.new(|cx| CommandState::new(window, cx)),
            selected,
        }
    }

    /// Follows entry `index`: closes the dialog, puts focus back on the view
    /// the picker was opened from, then dispatches from there.
    fn follow(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        let action = FollowReference {
            context_name: self.context_name.clone(),
            target: entry.target.clone(),
        };
        if matches!(window.root::<Root>(), Some(Some(_))) {
            window.close_dialog(cx);
        }
        self.return_focus.focus(window, cx);
        window.defer(cx, move |window, cx| {
            window.dispatch_action(Box::new(action), cx);
        });
    }
}

impl Render for GoToPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let items: Vec<CommandItem> = self
            .entries
            .iter()
            .map(|entry| CommandItem::new().label(entry.label()))
            .collect();
        CommandList::new(&self.state)
            .items(items)
            .placeholder("Go to…")
            // Arrows and typing to filter move the selection; hover moves only
            // `Command`'s own highlight, never this.
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
            // Enter follows the selection; a click follows the row clicked.
            // `Command` reports its own (possibly hovered) highlight for both,
            // so the keyboard case reads the picker's selection instead.
            .on_confirm(move |index_path, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let index = if window.last_input_was_keyboard() {
                        this.selected.unwrap_or(index_path.row)
                    } else {
                        index_path.row
                    };
                    this.follow(index, window, cx);
                });
            })
    }
}
