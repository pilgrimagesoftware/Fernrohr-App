//! The entry dialog (`exec-command-library` D5): Name, Command, the target -
//! any pod, a pod selector (namespace, selector) or a workload (kind,
//! namespace, name) - and Container, with each validation error beside its
//! field. Save (Enter), Save and Run (`cmd-enter`) and Cancel (Escape), each
//! showing its key. What it draws is [`render`]'s.
//!
//! It opens on the first empty required field; Tab and Shift-Tab move through
//! every field and button. Saving checks the entry as the library would and
//! stays open with the errors when it's refused. Escape closes it, and the
//! dialog puts focus back where it was. Save and Run runs the saved entry where
//! the dialog came from: the library's context, or the resource it was saved
//! from.

use super::run::{self, Place};
use super::store::ExecLibraryStore;
use super::window::{self, Opener};
use crate::config::exec_library::{Entry, EntryError, Field, Target, WorkloadKind};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

pub(crate) mod render;
#[cfg(test)]
mod tests;

/// Where the dialog was opened from, for Save and Run.
#[derive(Clone, Debug)]
pub(crate) enum Origin {
    /// The Exec Library window.
    Library(Opener),
    /// Save to Library… on a resource, in `context`.
    Resource { context: String, place: Place },
}

/// Which kind of target the dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetKind {
    Any,
    Selector,
    Workload,
}

/// Opens the dialog on `entry` - editing it when `editing`, else a new entry
/// it prefills (or a blank one, with `None`).
pub(crate) fn open_with(
    entry: Option<Entry>,
    editing: bool,
    origin: Origin,
    window: &mut Window,
    cx: &mut App,
) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let title = if editing { "Edit Entry" } else { "New Entry" };
    let dialog = cx.new(|cx| EntryDialog::new(entry, editing, origin, window, cx));
    let body = dialog.clone();
    window.open_dialog(cx, move |built, window, cx| {
        let (ok, footer_dialog) = (body.clone(), body.clone());
        built
            .title(title)
            .w(px(520.))
            .content({
                let body = body.clone();
                move |content, _window, _cx| content.child(body.clone())
            })
            .on_ok(move |_event, window, cx| {
                ok.update(cx, |dialog, cx| dialog.save(false, window, cx));
                false
            })
            .footer(render::footer(&footer_dialog, window, cx))
    });
    dialog.update(cx, |dialog, cx| dialog.focus_first_empty(window, cx));
}

/// [`open_with`] for New (`None`) or Edit (`Some`).
pub(crate) fn open(entry: Option<Entry>, origin: Origin, window: &mut Window, cx: &mut App) {
    let editing = entry.is_some();
    open_with(entry, editing, origin, window, cx);
}

pub(crate) struct EntryDialog {
    /// The name of the entry being edited, or `None` for a new one.
    editing: Option<String>,
    origin: Origin,
    pub(super) name: Entity<InputState>,
    pub(super) command: Entity<InputState>,
    pub(super) namespace: Entity<InputState>,
    pub(super) selector: Entity<InputState>,
    pub(super) workload_name: Entity<InputState>,
    pub(super) container: Entity<InputState>,
    pub(super) target_kind: TargetKind,
    pub(super) workload_kind: WorkloadKind,
    /// The last save's errors, each beside its field.
    pub(super) errors: Vec<(Field, String)>,
    /// A refusal that belongs to no field: a read-only library.
    pub(super) general_error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EntryDialog {
    fn new(
        entry: Option<Entry>,
        editing: bool,
        origin: Origin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input =
            |text: &str, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
                let text = text.to_string();
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(text)
                })
            };
        let entry = entry.unwrap_or(Entry {
            name: String::new(),
            command: String::new(),
            container: None,
            target: Target::Any,
        });
        let (target_kind, namespace, selector, workload_kind, workload_name) = match &entry.target {
            Target::Any => (
                TargetKind::Any,
                String::new(),
                String::new(),
                WorkloadKind::Deployment,
                String::new(),
            ),
            Target::Selector {
                namespace,
                selector,
            } => (
                TargetKind::Selector,
                namespace.clone().unwrap_or_default(),
                selector.clone(),
                WorkloadKind::Deployment,
                String::new(),
            ),
            Target::Workload {
                namespace,
                workload_kind,
                name,
            } => (
                TargetKind::Workload,
                namespace.clone(),
                String::new(),
                *workload_kind,
                name.clone(),
            ),
        };
        let name = input(&entry.name, "Stream info", window, cx);
        let command = input(&entry.command, "nats stream info my-stream", window, cx);
        let namespace = input(&namespace, "my-namespace", window, cx);
        let selector = input(&selector, "app.kubernetes.io/name=my-app", window, cx);
        let workload_name = input(&workload_name, "my-app", window, cx);
        let container = input(
            entry.container.as_deref().unwrap_or_default(),
            "the pod's default",
            window,
            cx,
        );
        let subscriptions = [
            &name,
            &command,
            &namespace,
            &selector,
            &workload_name,
            &container,
        ]
        .into_iter()
        .map(|input| {
            cx.subscribe_in(input, window, |this: &mut Self, _, event, _, cx| {
                if let InputEvent::Change = event {
                    this.errors.clear();
                    this.general_error = None;
                    cx.notify();
                }
            })
        })
        .collect();
        Self {
            editing: editing.then(|| entry.name.clone()),
            origin,
            name,
            command,
            namespace,
            selector,
            workload_name,
            container,
            target_kind,
            workload_kind,
            errors: Vec::new(),
            general_error: None,
            _subscriptions: subscriptions,
        }
    }

    fn value(input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().to_string()
    }

    /// Focuses the first empty required field - Name, then Command - else Name.
    fn focus_first_empty(&self, window: &mut Window, cx: &mut Context<Self>) {
        let first = [&self.name, &self.command]
            .into_iter()
            .find(|input| Self::value(input, cx).trim().is_empty())
            .unwrap_or(&self.name);
        let focus = first.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// The entry the fields describe.
    fn entry(&self, cx: &App) -> Entry {
        let namespace = Self::value(&self.namespace, cx);
        let target = match self.target_kind {
            TargetKind::Any => Target::Any,
            TargetKind::Selector => Target::Selector {
                namespace: Some(namespace),
                selector: Self::value(&self.selector, cx),
            },
            TargetKind::Workload => Target::Workload {
                namespace,
                workload_kind: self.workload_kind,
                name: Self::value(&self.workload_name, cx),
            },
        };
        Entry {
            name: Self::value(&self.name, cx),
            command: Self::value(&self.command, cx),
            container: Some(Self::value(&self.container, cx)),
            target,
        }
    }

    pub(super) fn set_target_kind(&mut self, kind: TargetKind, cx: &mut Context<Self>) {
        self.target_kind = kind;
        self.errors.clear();
        cx.notify();
    }

    pub(super) fn set_workload_kind(&mut self, kind: WorkloadKind, cx: &mut Context<Self>) {
        self.workload_kind = kind;
        cx.notify();
    }

    pub(super) fn error_for(&self, field: Field) -> Option<String> {
        self.errors
            .iter()
            .find(|(held, _)| *held == field)
            .map(|(_, message)| message.clone())
    }

    /// Saves the entry - and with `then_run`, runs it where the dialog came
    /// from - or shows why not and stays open.
    pub(super) fn save(&mut self, then_run: bool, window: &mut Window, cx: &mut Context<Self>) {
        let editing = self.editing.clone();
        let checked = ExecLibraryStore::get(cx).check(self.entry(cx), editing.as_deref());
        let saved = checked.and_then(|entry| {
            ExecLibraryStore::update(cx, |library| match &editing {
                Some(name) => library.update(name, entry.clone()),
                None => library.create(entry.clone()),
            })
            .map(|()| entry)
        });
        let entry = match saved {
            Ok(entry) => entry,
            Err(error) => return self.refused(error, cx),
        };
        window.close_dialog(cx);
        if then_run {
            let origin = self.origin.clone();
            window.defer(cx, move |window, cx| match origin {
                Origin::Library(opener) => window::run_in_context(entry, opener, window, cx),
                Origin::Resource { context, place } => run::run(entry, context, place, window, cx),
            });
        }
    }

    fn refused(&mut self, error: EntryError, cx: &mut Context<Self>) {
        match error.field() {
            Some(field) => self.errors = vec![(field, error.to_string())],
            None => self.general_error = Some(error.to_string()),
        }
        cx.notify();
    }
}
