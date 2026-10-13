//! What the Report Issue window draws: the type, both fields, the log option
//! (bugs only), the screenshots, the diagnostics, how Report will file, and
//! Close and Report with their keys.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Enter, Input, Textarea};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _};
use gpui_kit::*;

use super::body::{IssueKind, LogSection, file_name};
use super::window::{ReportIssueWindow, Route, close};
use super::{AddScreenshots, CloseReport, KEY_CONTEXT, SendReport};
use crate::consts::REPORT_MAX_SCREENSHOTS;

/// Element ids, for tests.
pub(super) const REPORT_BUTTON: &str = "report-issue-report";
pub(super) const CLOSE_BUTTON: &str = "report-issue-close";
pub(super) const LOG_CHECKBOX: &str = "report-issue-attach-log";
pub(super) const LOG_PREVIEW: &str = "report-issue-log-preview";
pub(super) const ROUTE_LINE: &str = "report-issue-route";

impl Render for ReportIssueWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let space = crate::ui::space::spacing(cx);
        let entity = cx.entity();
        let enabled = self.can_send(cx);

        let label = |text: &'static str| {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(text)
        };
        let route_line = match self.route {
            Route::Checking => "Checking for the GitHub CLI\u{2026}",
            Route::Gh => "Report files the issue with the GitHub CLI (gh).",
            Route::Browser => "Report opens a prefilled issue in your browser.",
        };

        div()
            .id("report-issue-window")
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &SendReport, window, cx| this.send(window, cx)))
            .on_action(cx.listener(|_, _: &CloseReport, window, cx| close(window, cx)))
            .on_action(cx.listener(|this, _: &AddScreenshots, _, cx| this.choose_screenshots(cx)))
            // ⌘↵ in a field is the field's own Enter, which a text area
            // takes as a newline before any binding of ours is reached - so
            // it is caught on the way down and turned into Report.
            .capture_action(cx.listener(|this, enter: &Enter, window, cx| {
                if enter.secondary && !enter.shift {
                    cx.stop_propagation();
                    this.send(window, cx);
                }
            }))
            .size_full()
            .overflow_y_scroll()
            .p(space.panel_inset)
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(kind_selector(&entity, self.kind))
            .child(label("Subject"))
            .child(Input::new(&self.subject).w_full())
            .child(label("Description"))
            .child(Textarea::new(&self.description).w_full().h(px(160.)))
            .children(
                self.kind
                    .offers_log()
                    .then(|| log_option(&entity, self, &theme)),
            )
            .child(screenshots_section(&entity, self, &theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.))
                    .bg(theme.muted)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Version {}", self.diagnostics.version))
                    .child(format!("Build {}", self.diagnostics.build))
                    .child(format!("Platform {}", self.diagnostics.platform)),
            )
            .child(
                div()
                    .id(ROUTE_LINE)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(route_line),
            )
            .child(footer(enabled, self.sending, window))
    }
}

fn kind_selector(entity: &Entity<ReportIssueWindow>, selected: IssueKind) -> RadioGroup {
    let entity = entity.clone();
    RadioGroup::horizontal("report-issue-kind")
        .children(IssueKind::ALL.map(|kind| Radio::new(kind.label()).label(kind.label())))
        .selected_index(IssueKind::ALL.iter().position(|kind| *kind == selected))
        .on_click(move |index, window, cx| {
            if let Some(&kind) = IssueKind::ALL.get(*index) {
                entity.update(cx, |this, cx| this.set_kind(kind, window, cx));
            }
        })
}

fn log_option(
    entity: &Entity<ReportIssueWindow>,
    report: &ReportIssueWindow,
    theme: &gpui_kit::component::Theme,
) -> Div {
    let entity = entity.clone();
    let muted = |text: SharedString| {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text)
    };
    // Checked, the option shows exactly what it attaches.
    let preview = report.attach_log.then(|| match &report.log_preview {
        None => muted("Reading the log\u{2026}".into()).into_any_element(),
        Some(LogSection::Path(path)) => muted(
            format!(
                "The log couldn't be read, so the report names {} instead.",
                path.display()
            )
            .into(),
        )
        .into_any_element(),
        Some(LogSection::Tail(tail)) => div()
            .id(LOG_PREVIEW)
            .test_support()
            .max_h(px(160.))
            .overflow_y_scroll()
            .p_2()
            .rounded(px(6.))
            .bg(theme.muted)
            .font_family(theme.mono_font_family.clone())
            .text_xs()
            .child(gpui_kit::base::SelectableText::new(
                "report-issue-log-preview-text",
                if tail.trim().is_empty() {
                    "(The log is empty.)".to_string()
                } else {
                    tail.clone()
                },
            ))
            .into_any_element(),
    });
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            Checkbox::new(LOG_CHECKBOX)
                .label("Attach the application log")
                .checked(report.attach_log)
                .on_click(move |checked, _, cx| {
                    entity.update(cx, |this, cx| this.set_attach_log(*checked, cx));
                }),
        )
        .child(muted(
            "Attaches the end of the log, shown below once checked. Logs can include context \
             names, hosts and paths."
                .into(),
        ))
        .children(preview)
}

/// The chosen screenshots by name, each removable, the add control, and a
/// note when the last pick skipped files.
fn screenshots_section(
    entity: &Entity<ReportIssueWindow>,
    report: &ReportIssueWindow,
    theme: &gpui_kit::component::Theme,
) -> Div {
    let add = {
        let entity = entity.clone();
        Button::new("report-issue-add-screenshots")
            .icon(IconName::Plus)
            .label("Add Screenshots\u{2026}")
            .ghost()
            .small()
            .disabled(report.screenshots.len() >= REPORT_MAX_SCREENSHOTS)
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.choose_screenshots(cx)))
    };
    let rows = report.screenshots.iter().enumerate().map(|(index, path)| {
        let name = file_name(path);
        let entity = entity.clone();
        let path = path.clone();
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!(
                    "report-issue-remove-screenshot-{index}"
                )))
                .icon(IconName::Close)
                .ghost()
                .xsmall()
                .tooltip(format!("Remove {name}"))
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.remove_screenshot(&path, cx));
                }),
            )
            .child(div().min_w_0().truncate().text_sm().child(name))
    });
    let skipped = (report.skipped > 0).then(|| {
        let files = if report.skipped == 1 { "file" } else { "files" };
        div().text_xs().text_color(theme.muted_foreground).child(format!(
            "Skipped {} {files}: only PNG, JPEG and GIF images, up to {REPORT_MAX_SCREENSHOTS}.",
            report.skipped
        ))
    });
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Screenshots"),
        )
        .children(rows)
        .child(div().flex().child(add))
        .children(skipped)
}

/// Close and Report, each showing its key from the live keymap.
fn footer(enabled: bool, sending: bool, window: &mut Window) -> Div {
    let key = |action: &dyn Action| Kbd::binding_for_action(action, Some(KEY_CONTEXT), window);
    div()
        .flex()
        .justify_end()
        .gap_2()
        .child(
            Button::new(CLOSE_BUTTON)
                .label("Close")
                .ghost()
                .children(key(&CloseReport))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(CloseReport), cx)),
        )
        .child(
            Button::new(REPORT_BUTTON)
                .label(if sending {
                    "Reporting\u{2026}"
                } else {
                    "Report"
                })
                .primary()
                .loading(sending)
                .disabled(!enabled)
                .children(key(&SendReport))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(SendReport), cx)),
        )
}
