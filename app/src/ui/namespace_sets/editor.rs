//! The set editor (`namespace-sets` 2.2, 3.1-3.3): a name field over the
//! filterable namespace list (`ui::namespace_filter`, without "All
//! namespaces"), each namespace checked while it's in the set.
//!
//! Creating starts from the focused list's namespaces and saves on Create (or
//! Enter in the name field); dismissing it makes nothing. Editing saves each
//! namespace added or removed at once, and Save applies a rename. The list is
//! the connected cluster's namespaces plus any the set holds that the cluster
//! doesn't have, marked as absent, so a set made on another cluster can still
//! be edited here. Refusals - a taken or empty name, an empty set - are said in
//! the dialog. Escape clears the list's filter first, then closes.

use super::store::NamespaceSets;
use super::{CREATE_KEY, CreateNamespaceSet, EDIT_KEY, EditNamespaceSet};
use crate::config::namespaces::SetError;
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::ui::namespace_filter::{NamespaceFilter, OnPick, OnQuery};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Enter, Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;
use std::rc::Rc;

/// Whether the editor makes a set or changes one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorMode {
    Create,
    /// The set's name when the editor opened - or since its last rename.
    Edit {
        name: String,
    },
}

/// The name field's element id.
pub const NAME_INPUT_ID: &str = "namespace-set-name";
/// The Create / Save button.
pub const SAVE_ID: &str = "namespace-set-save";
/// The debug selector of the refusal line.
pub const ERROR_SELECTOR: &str = "namespace-set-error";

/// Opens the editor in `mode` over `cluster`'s namespaces. A new set starts
/// with `seed`.
pub fn open(
    mode: EditorMode,
    seed: Vec<String>,
    cluster: Entity<NamespaceList>,
    window: &mut Window,
    cx: &mut App,
) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let editor = cx.new(|cx| SetEditor::new(mode.clone(), seed, cluster, window, cx));
    let name_input = editor.read(cx).name_input.clone();
    let (title, action, fallback): (&str, &dyn Action, &str) = match mode {
        EditorMode::Create => ("Create Namespace Set", &CreateNamespaceSet, CREATE_KEY),
        EditorMode::Edit { .. } => ("Edit Namespace Set", &EditNamespaceSet, EDIT_KEY),
    };
    // The command's live key, beside the title.
    let key = Kbd::binding_for_action(action, None, window)
        .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("a valid default key")));
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let editor = editor.clone();
        dialog
            .w(px(480.))
            .title(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(title)
                    .child(key.clone()),
            )
            .content(move |content, _window, _cx| content.child(editor.clone()))
    });
    let focus = name_input.read(cx).focus_handle(cx);
    window.focus(&focus, cx);
}

pub struct SetEditor {
    mode: EditorMode,
    name_input: Entity<InputState>,
    filter: NamespaceFilter,
    cluster: Entity<NamespaceList>,
    /// A new set's namespaces; an edited set's live in the store.
    members: Vec<String>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl SetEditor {
    fn new(
        mode: EditorMode,
        seed: Vec<String>,
        cluster: Entity<NamespaceList>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = match &mode {
            EditorMode::Create => String::new(),
            EditorMode::Edit { name } => name.clone(),
        };
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Set name")
                .default_value(name)
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &name_input,
                window,
                |this: &mut Self, _, event, _window, cx| {
                    if let InputEvent::Change = event {
                        this.error = None;
                        cx.notify();
                    }
                },
            ),
            cx.observe(&cluster, |_, _, cx| cx.notify()),
        ];
        let mut members = seed;
        members.sort_unstable();
        members.dedup();
        Self {
            mode,
            name_input,
            filter: NamespaceFilter::without_all(window, cx),
            cluster,
            members,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// The set's namespaces as they stand.
    fn members(&self, cx: &App) -> Vec<String> {
        match &self.mode {
            EditorMode::Create => self.members.clone(),
            EditorMode::Edit { name } => NamespaceSets::get(cx)
                .find(name)
                .map(|set| set.namespaces.clone())
                .unwrap_or_default(),
        }
    }

    /// A toggle in the list: `next` is the set's namespaces after it. An
    /// edited set saves it at once.
    fn pick(&mut self, next: Vec<String>, cx: &mut Context<Self>) {
        self.error = None;
        match &self.mode {
            EditorMode::Create => self.members = next,
            EditorMode::Edit { name } => {
                let current = self.members(cx);
                let added = next.iter().find(|ns| !current.contains(ns));
                let removed = current.iter().find(|ns| !next.contains(ns));
                let change = match (added, removed) {
                    (Some(namespace), _) => Some((namespace.clone(), true)),
                    (None, Some(namespace)) => Some((namespace.clone(), false)),
                    (None, None) => None,
                };
                if let Some((namespace, add)) = change {
                    let name = name.clone();
                    if let Err(error) =
                        NamespaceSets::update(cx, |sets| sets.apply_to(&name, &namespace, add))
                    {
                        self.error = Some(error.to_string().into());
                    }
                }
            }
        }
        cx.notify();
    }

    /// Create: saves the new set. Edit: applies a rename. Closes on success.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().to_string();
        let result: Result<(), SetError> = match &self.mode {
            EditorMode::Create => {
                let members = self.members.clone();
                NamespaceSets::update(cx, |sets| sets.create(&name, members))
            }
            EditorMode::Edit { name: current } => {
                let current = current.clone();
                NamespaceSets::update(cx, |sets| sets.rename(&current, &name))
            }
        };
        match result {
            Ok(()) => window.close_dialog(cx),
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
            }
        }
    }
}

impl Render for SetEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let members = self.members(cx);
        let cluster: Vec<String> = self.cluster.read(cx).names().to_vec();
        // What the set holds that this cluster doesn't - only once the
        // cluster's list is in, or everything would read as absent.
        let absent: Vec<String> = if cluster.is_empty() {
            Vec::new()
        } else {
            members
                .iter()
                .filter(|namespace| !cluster.contains(namespace))
                .cloned()
                .collect()
        };
        let mut shown = cluster;
        shown.extend(members.iter().cloned());
        shown.sort_unstable();
        shown.dedup();
        let on_pick: OnPick = Rc::new({
            let this = this.clone();
            move |next, _window, cx| {
                let _ = this.update(cx, |editor, cx| editor.pick(next, cx));
            }
        });
        let on_query: OnQuery = Rc::new({
            let this = this.clone();
            move |_window, cx| {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        });
        let save_label = match self.mode {
            EditorMode::Create => "Create",
            EditorMode::Edit { .. } => "Save",
        };
        let danger = cx.theme().danger;
        div()
            .flex()
            .flex_col()
            .gap_3()
            // Enter in the name saves. The input lets Enter bubble on, and
            // the dialog would take it as its own confirm and close, so it
            // stops here.
            .child(
                div()
                    .on_action(cx.listener(|this, _: &Enter, window, cx| this.save(window, cx)))
                    .child(Input::new(&self.name_input).id(NAME_INPUT_ID)),
            )
            .child(
                self.filter
                    .element_marked(&shown, &members, &absent, on_pick, on_query, cx),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .debug_selector(|| ERROR_SELECTOR.into())
                    .text_sm()
                    .text_color(danger)
                    .child(error)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("namespace-set-cancel")
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(SAVE_ID)
                            .label(save_label)
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
