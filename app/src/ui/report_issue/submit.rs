//! Report: through `gh` when it's ready, else - or when it fails - in the
//! browser, then the screenshots shown and the outcome told in a main window,
//! since the Report Issue window closes once the report is handed off.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::body::{LogSection, fields_filled, github_issue_url, issue_body};
use super::window::{ReportIssueWindow, Route, close};
use super::{gh, gh_path, log_path, resolve_path, screenshots};

/// Finds out, off the main thread, whether `gh` is ready, and redraws the
/// window to say which way Report will go.
pub(super) fn check_route(cx: &mut Context<ReportIssueWindow>) {
    let Some(path) = gh_path(cx) else {
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                this.route = Route::Browser;
                cx.notify();
            });
        })
        .detach();
        return;
    };
    let ready = cx.background_spawn(async move { gh::ready(&resolve_path(path)) });
    cx.spawn(async move |this, cx| {
        let route = if ready.await {
            Route::Gh
        } else {
            Route::Browser
        };
        let _ = this.update(cx, |this, cx| {
            this.route = route;
            cx.notify();
        });
    })
    .detach();
}

impl ReportIssueWindow {
    pub(super) fn can_send(&self, cx: &App) -> bool {
        !self.sending
            && fields_filled(
                &self.subject.read(cx).value(),
                &self.description.read(cx).value(),
            )
    }

    /// Report: hands the report off, then closes the window.
    pub(super) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let subject = self.subject.read(cx).value().trim().to_string();
        let description = self.description.read(cx).value().to_string();
        let kind = self.kind;
        let shots = self.screenshots.clone();
        let diagnostics = self.diagnostics.clone();
        // The log goes as it was shown; if it hasn't been read yet, only
        // where it is - never text the reporter hasn't seen.
        let log = (self.attach_log && kind.offers_log()).then(|| {
            self.log_preview
                .clone()
                .unwrap_or_else(|| LogSection::Path(log_path(cx)))
        });
        // The browser page can't carry the log's text, only where it is.
        let browser_log = log.as_ref().map(|_| LogSection::Path(log_path(cx)));
        let browser_body = issue_body(&description, &shots, browser_log.as_ref(), &diagnostics);
        let browser_url = github_issue_url(&subject, &browser_body, kind);

        let (Route::Gh, Some(path)) = (self.route, gh_path(cx)) else {
            cx.open_url(&browser_url);
            screenshots::reveal(shots);
            close(window, cx);
            return;
        };

        self.sending = true;
        cx.notify();
        let has_shots = !shots.is_empty();
        let body = issue_body(&description, &shots, log.as_ref(), &diagnostics);
        let filed = cx.background_spawn(async move {
            gh::create_labeled(&resolve_path(path), &subject, &body, kind.github_label())
        });
        cx.spawn_in(window, async move |_, cx| {
            let filed = filed.await;
            let _ = cx.update(|window, cx| {
                let notification = match filed {
                    Ok(issue) => {
                        // The screenshots are dropped onto the issue itself.
                        if has_shots {
                            cx.open_url(&issue);
                        }
                        Notification::success(format!("Filed {issue}")).title("Issue reported")
                    }
                    Err(reason) => {
                        cx.open_url(&browser_url);
                        Notification::warning(format!(
                            "The GitHub CLI couldn't file it ({reason}), so it opened in \
                             your browser instead."
                        ))
                        .title("Report Issue")
                    }
                };
                screenshots::reveal(shots);
                close(window, cx);
                notify_main_window(notification, cx);
            });
        })
        .detach();
    }
}

/// Shows `notification` in the active main window, or the first one open.
/// Deferred: the Report Issue window is closing as this is called.
fn notify_main_window(notification: Notification, cx: &mut App) {
    cx.defer(move |cx| {
        let main = |handle: &AnyWindowHandle| crate::util::shell::is_main_window(*handle, cx);
        let target = cx
            .active_window()
            .filter(main)
            .or_else(|| cx.windows().into_iter().find(main));
        if let Some(target) = target {
            let _ = target.update(cx, |_, window, cx| {
                window.push_notification(notification, cx)
            });
        }
    });
}
