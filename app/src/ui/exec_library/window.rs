//! The Exec Library window (`exec-command-library` D5): one window, like
//! Settings, listing the entries under a filter, to run, create, edit,
//! duplicate and delete them. What it draws is [`render`]'s.
//!
//! Run uses the context of the cluster panel the window was opened from -
//! recorded each time it is opened or raised - and, with none, asks which
//! connected context to use. A library file that didn't parse leaves the window
//! read-only, with the file and the error in a banner.

use super::chooser::{self, Choice};
use super::commands::OpenExecLibrary;
use super::run::{self, Place};
use super::store::ExecLibraryStore;
use crate::config::exec_library::Entry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::component::Root;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

pub(crate) mod render;
#[cfg(test)]
mod tests;

/// Where the window was opened from: the main window and its focused
/// cluster panel's context, if any.
#[derive(Clone, Debug, Default)]
pub(crate) struct Opener {
    pub(crate) main: Option<AnyWindowHandle>,
    pub(crate) context: Option<String>,
}

/// The one Exec Library window, if open.
struct LibraryWindowHandle(Option<WindowHandle<Root>>);

impl Global for LibraryWindowHandle {}

/// Opens the window on [`OpenExecLibrary`], from wherever it is dispatched.
pub(crate) fn init(cx: &mut App) {
    cx.on_action(|_: &OpenExecLibrary, cx: &mut App| {
        // Deferred: the action runs mid-update of the window it came from,
        // which has to be read for its focused panel.
        cx.defer(|cx| {
            let opener = cx.active_window().map_or_else(Opener::default, |main| {
                let context = main
                    .update(cx, |_, window, cx| {
                        crate::util::shell::focused_context(window, cx)
                    })
                    .ok()
                    .flatten();
                Opener {
                    main: crate::util::shell::is_main_window(main, cx).then_some(main),
                    context,
                }
            });
            open_or_focus(opener, cx);
        });
    });
}

/// Opens the window - or raises the open one - remembering `opener` for Run.
pub(crate) fn open_or_focus(opener: Opener, cx: &mut App) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx
        .try_global::<LibraryWindowHandle>()
        .and_then(|handle| handle.0)
        && cx.windows().contains(&handle.into())
    {
        let _ = handle.update(cx, |root, window, cx| {
            if let Ok(view) = root.view().clone().downcast::<LibraryWindow>() {
                view.update(cx, |view, _| view.opener = opener);
            }
            window.activate_window();
        });
        return Some(handle);
    }
    let options = WindowOptions {
        app_id: Some(crate::consts::APP_ID.into()),
        titlebar: Some(TitlebarOptions {
            title: Some("Exec Library".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            crate::consts::EXEC_LIBRARY_WINDOW_SIZE,
            cx,
        ))),
        window_min_size: Some(crate::consts::EXEC_LIBRARY_WINDOW_MIN_SIZE),
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        crate::ui::theme::watch_window(window, cx);
        let view = cx.new(|cx| LibraryWindow::new(opener, window, cx));
        let focus = view.read(cx).filter.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        view
    });
    let Ok((window, _)) = opened else {
        return None;
    };
    let handle = window.downcast::<Root>()?;
    let _ = handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(LibraryWindowHandle(None));
            true
        });
    });
    cx.set_global(LibraryWindowHandle(Some(handle)));
    Some(handle)
}

pub(crate) struct LibraryWindow {
    pub(crate) opener: Opener,
    pub(super) filter: Entity<InputState>,
    /// The highlighted row, among the filtered entries.
    pub(super) selected: usize,
    pub(crate) list_focus: FocusHandle,
    pub(super) focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl LibraryWindow {
    fn new(opener: Opener, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter entries…"));
        let subscriptions = vec![
            cx.subscribe_in(&filter, window, |this: &mut Self, _, event, _, cx| {
                if let InputEvent::Change = event {
                    this.selected = 0;
                    cx.notify();
                }
            }),
            cx.observe_global::<ExecLibraryStore>(|this, cx| {
                let count = this.entries(cx).len();
                this.selected = this.selected.min(count.saturating_sub(1));
                cx.notify();
            }),
        ];
        Self {
            opener,
            filter,
            selected: 0,
            list_focus: cx.focus_handle().tab_stop(true),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The entries the filter leaves, in library order: a name, command or
    /// scope containing the filter's text, ignoring case.
    pub(super) fn entries(&self, cx: &App) -> Vec<Entry> {
        let filter = self.filter.read(cx).value().trim().to_lowercase();
        ExecLibraryStore::get(cx)
            .entries
            .iter()
            .filter(|entry| {
                filter.is_empty()
                    || [&entry.name, &entry.command, &entry.target.describe()]
                        .iter()
                        .any(|text| text.to_lowercase().contains(&filter))
            })
            .cloned()
            .collect()
    }

    pub(super) fn selected_entry(&self, cx: &App) -> Option<Entry> {
        self.entries(cx).get(self.selected).cloned()
    }

    /// Moves the highlight `delta` rows, within the list.
    pub(super) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.entries(cx).len();
        if count == 0 {
            return;
        }
        let next = (self.selected as isize + delta).clamp(0, count as isize - 1);
        self.selected = next as usize;
        cx.notify();
    }

    /// Runs the highlighted entry: in the opener's context, or - with none -
    /// the connected context the user picks.
    pub(super) fn run_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        run_in_context(entry, self.opener.clone(), window, cx);
    }

    /// Duplicates the highlighted entry and highlights the copy.
    pub(super) fn duplicate_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        match ExecLibraryStore::update(cx, |library| library.duplicate(&entry.name)) {
            Ok(copy) => {
                if let Some(index) = self.entries(cx).iter().position(|e| e.name == copy) {
                    self.selected = index;
                }
                cx.notify();
            }
            Err(error) => run::report(&entry, &error.to_string(), window, cx),
        }
    }

    /// Asks before deleting the highlighted entry - irreversible, so the
    /// confirmation opens on Cancel.
    pub(super) fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::confirm_dialog::{self, Confirmation, Severity};
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        let name = entry.name.clone();
        confirm_dialog::open(
            Confirmation {
                title: "Delete Entry?".into(),
                body: crate::ui::confirm_text::ConfirmText::from("Delete ")
                    .name(&name)
                    .text(" from the exec library?"),
                confirm: "Delete".into(),
                id_prefix: DELETE_ID_PREFIX,
                severity: Severity::Irreversible,
            },
            move |_window, cx| {
                let name = name.clone();
                let _ = ExecLibraryStore::update(cx, move |library| Ok(library.remove(&name)));
            },
            window,
            cx,
        );
    }
}

/// The Delete confirmation's id prefix.
pub(crate) const DELETE_ID_PREFIX: &str = "exec-library-delete";

/// The title of the context chooser.
pub(crate) const CONTEXT_CHOOSER_TITLE: &str = "Run in Which Context?";

/// Runs `entry` from the library in `opener`'s context, or asks - in `window`
/// - which connected context to use when it has none.
pub(crate) fn run_in_context(entry: Entry, opener: Opener, window: &mut Window, cx: &mut App) {
    if let Some(context) = opener.context.clone() {
        return run_in(entry, context, opener.main, window, cx);
    }
    let connected: Vec<String> = ClusterRegistry::open_contexts(cx)
        .into_iter()
        .filter(|context| {
            ClusterRegistry::existing_connection(cx, context).is_some_and(|connection| {
                matches!(connection.read(cx).state, ConnectionState::Connected(_))
            })
        })
        .collect();
    if connected.is_empty() {
        return run::report(&entry, "No cluster is connected.", window, cx);
    }
    let choices = connected
        .iter()
        .map(|context| Choice {
            label: context.clone(),
            detail: None,
        })
        .collect();
    chooser::open(
        CONTEXT_CHOOSER_TITLE,
        choices,
        move |index, window, cx| {
            run_in(
                entry.clone(),
                connected[index].clone(),
                opener.main,
                window,
                cx,
            )
        },
        window,
        cx,
    );
}

/// Runs `entry` in `context`, opening its session in `main` - the window it
/// was opened from, else any main window - and bringing that window forward.
fn run_in(
    entry: Entry,
    context: String,
    main: Option<AnyWindowHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    let main = main.filter(|main| cx.windows().contains(main)).or_else(|| {
        cx.windows()
            .into_iter()
            .find(|handle| crate::util::shell::is_main_window(*handle, cx))
    });
    let Some(main) = main else {
        return run::report(
            &entry,
            "No Fernrohr window is open to run it in.",
            window,
            cx,
        );
    };
    let _ = main.update(cx, |_, window, cx| {
        window.activate_window();
        run::run(entry, context, Place::Library, window, cx);
    });
}

/// Test-only: the open window.
#[cfg(test)]
pub(crate) fn test_handle(cx: &App) -> Option<WindowHandle<Root>> {
    cx.try_global::<LibraryWindowHandle>()
        .and_then(|handle| handle.0)
}

/// Test-only: the open window's view.
#[cfg(test)]
pub(crate) fn test_view(cx: &mut App) -> Option<Entity<LibraryWindow>> {
    let handle = cx
        .try_global::<LibraryWindowHandle>()
        .and_then(|handle| handle.0)?;
    handle
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<LibraryWindow>().ok()
        })
        .ok()
        .flatten()
}
