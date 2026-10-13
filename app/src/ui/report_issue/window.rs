//! The one Report Issue window: opening it, or raising the open one, and the
//! state its form keeps - the type, both fields, the log choice, the
//! screenshots, and how Report will file.

use std::path::{Path, PathBuf};

use gpui_kit::component::Root;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;

use super::ReportError;
use super::body::{Diagnostics, IssueKind, LogSection};
use super::screenshots;
use crate::consts::{REPORT_WINDOW_MIN_SIZE, REPORT_WINDOW_SIZE};
use crate::util::app_log;

/// The open Report Issue window, if one is.
struct ReportWindowHandle(Option<WindowHandle<Root>>);

impl Global for ReportWindowHandle {}

/// Raises the open window, or opens one. `prefill` - an error's Report… -
/// fills an open window only while both its fields are empty, so a report
/// already started is never replaced.
pub(super) fn open_or_raise(prefill: Option<ReportError>, cx: &mut App) {
    if let Some(handle) = open_window(cx) {
        let _ = handle.update(cx, |root, window, cx| {
            if let (Some(prefill), Ok(view)) =
                (prefill, root.view().clone().downcast::<ReportIssueWindow>())
            {
                view.update(cx, |view, cx| view.prefill_if_empty(prefill, window, cx));
            }
            window.activate_window();
        });
        return;
    }
    let opened = gpui_kit::open_window(
        WindowOptions {
            app_id: Some(crate::consts::APP_ID.into()),
            titlebar: Some(TitlebarOptions {
                title: Some("Report Issue".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                REPORT_WINDOW_SIZE,
                cx,
            ))),
            window_min_size: Some(REPORT_WINDOW_MIN_SIZE),
            ..Default::default()
        },
        cx,
        |window, cx| {
            crate::ui::theme::watch_window(window, cx);
            let view = cx.new(|cx| ReportIssueWindow::new(prefill, window, cx));
            // #249: Subject has focus as the window opens.
            let subject = view.read(cx).subject.clone();
            subject.update(cx, |input, cx| input.focus(window, cx));
            view
        },
    );
    let window = match opened {
        Ok((window, _)) => window,
        Err(error) => {
            log::error!("failed to open the Report Issue window: {error}");
            return;
        }
    };
    let Some(handle) = window.downcast::<Root>() else {
        return;
    };
    let _ = handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(ReportWindowHandle(None));
            true
        });
    });
    cx.set_global(ReportWindowHandle(Some(handle)));
}

/// The open window's handle, if it is still open.
fn open_window(cx: &App) -> Option<WindowHandle<Root>> {
    cx.try_global::<ReportWindowHandle>()
        .and_then(|handle| handle.0)
        .filter(|handle| cx.windows().contains(&(*handle).into()))
}

/// Closes the window. Its handle goes with it: the should-close hook only
/// runs when the platform asks.
pub(super) fn close(window: &mut Window, cx: &mut App) {
    cx.set_global(ReportWindowHandle(None));
    window.remove_window();
}

/// How Report will file the issue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Route {
    /// Still finding out whether `gh` is ready; Report opens the browser
    /// until it knows.
    #[default]
    Checking,
    Gh,
    Browser,
}

pub(super) struct ReportIssueWindow {
    pub(super) kind: IssueKind,
    pub(super) subject: Entity<InputState>,
    pub(super) description: Entity<TextareaState>,
    /// Whether a bug report attaches the end of the application log. Off
    /// until the reporter turns it on: logs can name contexts and hosts.
    pub(super) attach_log: bool,
    /// What the log option attaches, read when it is checked and shown under
    /// it: the report carries exactly this, never text the reporter hasn't
    /// seen. `None` while it is being read, or while the option is off.
    pub(super) log_preview: Option<LogSection>,
    pub(super) screenshots: Vec<PathBuf>,
    /// How many files the last pick skipped, to say so; cleared once the list
    /// changes again.
    pub(super) skipped: usize,
    pub(super) route: Route,
    /// A report is on its way through `gh`.
    pub(super) sending: bool,
    pub(super) diagnostics: Diagnostics,
}

impl ReportIssueWindow {
    fn new(prefill: Option<ReportError>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let kind = IssueKind::default();
        let (subject_text, description_text) = prefill
            .map(|report| (report.subject, report.description))
            .unwrap_or_default();
        let subject = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(kind.subject_placeholder());
            input.set_value(subject_text, window, cx);
            input
        });
        let description = cx.new(|cx| {
            let mut input =
                TextareaState::new(window, cx).placeholder(kind.description_placeholder());
            input.set_value(description_text, window, cx);
            input
        });
        let this = Self {
            kind,
            subject,
            description,
            attach_log: false,
            log_preview: None,
            screenshots: Vec::new(),
            skipped: 0,
            route: Route::Checking,
            sending: false,
            diagnostics: Diagnostics::current(),
        };
        super::submit::check_route(cx);
        this
    }

    /// Switches the type. The text stays; only the placeholders follow it.
    pub(super) fn set_kind(
        &mut self,
        kind: IssueKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.kind = kind;
        self.subject.update(cx, |input, cx| {
            input.set_placeholder(kind.subject_placeholder(), window, cx);
        });
        self.description.update(cx, |input, cx| {
            input.set_placeholder(kind.description_placeholder(), window, cx);
        });
        cx.notify();
    }

    /// Turns the log option on or off. On, it reads the end of the log off
    /// the main thread, to show it and to attach just that.
    pub(super) fn set_attach_log(&mut self, attach: bool, cx: &mut Context<Self>) {
        self.attach_log = attach;
        self.log_preview = None;
        cx.notify();
        if !attach {
            return;
        }
        let path = super::log_path(cx);
        let read = cx.background_spawn(async move {
            match app_log::read_tail(&path) {
                Ok(tail) => LogSection::Tail(tail),
                Err(error) => {
                    log::warn!("couldn't read {} for a report: {error}", path.display());
                    LogSection::Path(path)
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let preview = read.await;
            let _ = this.update(cx, |this, cx| {
                // Unchecked again while it was read: drop it.
                if this.attach_log {
                    this.log_preview = Some(preview);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn add_screenshots(&mut self, picked: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.skipped = screenshots::add(&mut self.screenshots, picked);
        cx.notify();
    }

    pub(super) fn remove_screenshot(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.screenshots.retain(|chosen| chosen != path);
        self.skipped = 0;
        cx.notify();
    }

    /// Opens the system file picker, and adds what the user chooses.
    pub(super) fn choose_screenshots(&mut self, cx: &mut Context<Self>) {
        if self.screenshots.len() >= crate::consts::REPORT_MAX_SCREENSHOTS {
            return;
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add Screenshots".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(picked))) = picked.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.add_screenshots(picked, cx));
        })
        .detach();
    }

    /// Fills both fields from `prefill`, unless the reporter has typed in
    /// either - then the window is only raised.
    fn prefill_if_empty(
        &mut self,
        prefill: ReportError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typed = !self.subject.read(cx).value().trim().is_empty()
            || !self.description.read(cx).value().trim().is_empty();
        if typed {
            return;
        }
        self.set_kind(IssueKind::Bug, window, cx);
        self.subject
            .update(cx, |input, cx| input.set_value(prefill.subject, window, cx));
        self.description.update(cx, |input, cx| {
            input.set_value(prefill.description, window, cx)
        });
    }
}

/// Test-only: the open Report Issue window's view, if one is open.
#[cfg(test)]
pub(super) fn test_view(cx: &mut App) -> Option<(WindowHandle<Root>, Entity<ReportIssueWindow>)> {
    let handle = open_window(cx)?;
    let view = handle
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<ReportIssueWindow>().ok()
        })
        .ok()
        .flatten()?;
    Some((handle, view))
}
