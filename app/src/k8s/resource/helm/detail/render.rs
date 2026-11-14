//! Drawing the release detail panel: its tab bar, the Overview, History,
//! Notes and Resources tabs, the hint row, and the states that stand in for
//! them.

use super::commands::{
    CopyManifest, CopyValues, HISTORY_KEY, MANIFEST_KEY, NOTES_KEY, OPEN_RESOURCE_KEY,
    OVERVIEW_KEY, OpenResource, PANEL_KEY_CONTEXT, RESOURCES_KEY, REVEAL_KEY, ShowHistory,
    ShowManifest, ShowNotes, ShowOverview, ShowResources, ShowValues, ToggleReveal, VALUES_KEY,
    VIEW_MANIFEST_KEY, VIEW_VALUES_KEY, ViewManifest, ViewValues,
};
use super::fetch::Fetched;
use super::panel::{HelmReleasePanel, ReleaseTab};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::helm::actions::{
    self, BannerIds, ROLLBACK_KEY, RollbackRelease, UNINSTALL_KEY, UninstallRelease,
};
use crate::k8s::resource::helm::list::status_tone;
use crate::ui::key_hint_row::{KeyHint, KeyHintRow};
use crate::ui::panel_title;
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const HINTS_ID: &str = "helm-release-hints";

/// The action banner's ids (helm-releases section 8).
pub(crate) const BANNER: BannerIds = BannerIds {
    running: "helm-release-action-running",
    refusal: "helm-release-action-refusal",
    dismiss: "helm-release-action-dismiss",
};

fn tab_action(tab: ReleaseTab) -> (Box<dyn Action>, &'static str) {
    match tab {
        ReleaseTab::Overview => (Box::new(ShowOverview), OVERVIEW_KEY),
        ReleaseTab::History => (Box::new(ShowHistory), HISTORY_KEY),
        ReleaseTab::Values => (Box::new(ShowValues), VALUES_KEY),
        ReleaseTab::Manifest => (Box::new(ShowManifest), MANIFEST_KEY),
        ReleaseTab::Notes => (Box::new(ShowNotes), NOTES_KEY),
        ReleaseTab::Resources => (Box::new(ShowResources), RESOURCES_KEY),
    }
}

impl HelmReleasePanel {
    fn tab_bar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let space = crate::ui::space::spacing(cx);
        let mut bar = div().flex().items_center().gap(space.control_gap);
        for tab in ReleaseTab::ALL {
            let (action, _) = tab_action(tab);
            let key = Kbd::binding_for_action(action.as_ref(), Some(PANEL_KEY_CONTEXT), window);
            let id = format!("helm-release-tab-{}", tab.id());
            let selector = id.clone();
            let this = cx.weak_entity();
            let button = Button::new(SharedString::from(id))
                .label(tab.title())
                .children(key)
                .xsmall()
                .debug_selector(move || selector.clone())
                .on_click(move |_, _window, cx| {
                    let _ = this.update(cx, |this, cx| this.show(tab, cx));
                });
            bar = bar.child(if tab == self.tab {
                button.primary()
            } else {
                button.ghost()
            });
        }
        bar
    }

    fn field(label: &'static str, value: String, cx: &App) -> Div {
        use crate::ui::typography::TypeRole as _;
        div()
            .flex()
            .gap_2()
            .child(
                div()
                    .w(px(140.))
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .data_font()
                    .debug_selector(move || format!("helm-release-field-{label}"))
                    .child(value),
            )
    }

    fn overview(&self, cx: &App) -> AnyElement {
        let history = self.history(cx);
        let Some(latest) = history.first() else {
            return div().into_any_element();
        };
        let detail = self.detail(latest.version);
        let text = |value: Option<&str>| value.unwrap_or("").to_string();
        let chart = detail.map(|detail| format!("{}-{}", detail.chart.name, detail.chart.version));
        let fields = [
            ("Name", self.key.name.clone()),
            ("Namespace", self.key.namespace.clone()),
            ("Status", latest.status.clone()),
            ("Revision", latest.version.to_string()),
            ("Chart", chart.unwrap_or_default()),
            (
                "App version",
                text(detail.and_then(|detail| detail.chart.app_version.as_deref())),
            ),
            (
                "First deployed",
                text(detail.and_then(|detail| detail.info.first_deployed.as_deref())),
            ),
            (
                "Last deployed",
                text(detail.and_then(|detail| detail.info.last_deployed.as_deref())),
            ),
            (
                "Description",
                text(detail.and_then(|detail| detail.info.description.as_deref())),
            ),
        ];
        let mut list = div().flex().flex_col().gap_1();
        if let Some(Fetched::Unreadable(reason)) = self.fetched(latest.version) {
            list = list.child(
                div()
                    .text_color(crate::ui::style::status(crate::ui::style::Tone::Bad, cx))
                    .child(format!(
                        "Revision {} couldn't be read: {reason}",
                        latest.version
                    )),
            );
        }
        fields
            .into_iter()
            .fold(list, |list, (label, value)| {
                list.child(Self::field(label, value, cx))
            })
            .into_any_element()
    }

    fn history_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::ui::typography::TypeRole as _;
        let selected = self.selected_revision(cx);
        let mut rows = div().flex().flex_col().gap_0p5();
        for revision in self.history(cx) {
            let (chart, app_version, description) = match self.fetched(revision.version) {
                Some(Fetched::Ready(detail)) => (
                    format!("{}-{}", detail.chart.name, detail.chart.version),
                    detail.chart.app_version.clone().unwrap_or_default(),
                    detail.info.description.clone().unwrap_or_default(),
                ),
                Some(Fetched::Unreadable(_)) => {
                    ("couldn't be read".into(), String::new(), String::new())
                }
                _ => ("\u{2026}".into(), String::new(), String::new()),
            };
            let updated = self
                .detail(revision.version)
                .and_then(|detail| detail.info.last_deployed.clone())
                .unwrap_or_default();
            let deployed = revision.status == "deployed";
            let version = revision.version;
            let this = cx.weak_entity();
            let id = format!("helm-release-revision-{version}");
            let selector = id.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .gap_3()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .data_font()
                    .when(selected == Some(version), |row| {
                        row.bg(crate::ui::style::accent_subtle(cx))
                    })
                    .on_click(move |_, _window, cx| {
                        let _ = this.update(cx, |this, cx| this.select(version, cx));
                    })
                    .child(div().w(px(40.)).child(version.to_string()))
                    .child(
                        div()
                            .w(px(150.))
                            .text_color(crate::ui::style::status(status_tone(&revision.status), cx))
                            .child(if deployed {
                                format!("{} \u{2190} deployed", revision.status)
                            } else {
                                revision.status.clone()
                            }),
                    )
                    .child(div().w(px(200.)).child(updated))
                    .child(div().w(px(200.)).child(chart))
                    .child(div().w(px(100.)).child(app_version))
                    .child(div().flex_1().child(description)),
            );
        }
        rows.into_any_element()
    }

    /// The notes as plain, selectable monospace lines, as `helm get notes`
    /// prints them.
    /// Never Markdown: they are the chart author's text, and a fence or a
    /// link in them must show as written, not render.
    fn notes_tab(&self, cx: &App) -> AnyElement {
        match self.notes(cx) {
            Some(notes) if !notes.trim().is_empty() => {
                selectable_lines("helm-release-notes", notes)
                    .debug_selector(|| "helm-release-notes".into())
                    .into_any_element()
            }
            Some(_) => div().child("The chart has no notes.").into_any_element(),
            None => div().child("\u{2026}").into_any_element(),
        }
    }

    fn content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let space = crate::ui::space::spacing(cx);
        let status = |text: String| div().size_full().p(space.panel_inset).child(text);
        match &self.connection.read(cx).state {
            ConnectionState::Connecting => {
                return status("Connecting...".into()).into_any_element();
            }
            ConnectionState::WaitingForTunnel => {
                return status("Waiting for tunnel...".into()).into_any_element();
            }
            ConnectionState::Failed(reason) => {
                return panel_title::error_content(
                    "Couldn't connect to the cluster".to_string(),
                    Some(reason.clone()),
                    cx,
                )
                .into_any_element();
            }
            ConnectionState::Connected(_) => {}
        }
        if self.gone(cx) {
            return status(format!(
                "Helm release {} no longer exists in {}.",
                self.key.name, self.key.namespace
            ))
            .debug_selector(|| "helm-release-gone".into())
            .into_any_element();
        }
        let body = match self.tab {
            ReleaseTab::Overview => self.overview(cx),
            ReleaseTab::History => self.history_tab(cx),
            ReleaseTab::Values | ReleaseTab::Manifest => self.text_tab(self.tab, window, cx),
            ReleaseTab::Notes => self.notes_tab(cx),
            ReleaseTab::Resources => self.resources_tab(cx),
        };
        let mut hints = KeyHintRow::new(HINTS_ID)
            .hint_gap(space.control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground);
        for tab in ReleaseTab::ALL {
            let (action, fallback) = tab_action(tab);
            hints = hints.hint(KeyHint::for_action(
                action.as_ref(),
                Some(PANEL_KEY_CONTEXT),
                fallback,
                tab.title(),
                window,
            ));
        }
        let extra: &[(&dyn Action, &str, &'static str)] = match self.tab {
            ReleaseTab::History => &[
                (&ViewValues, VIEW_VALUES_KEY, "View values"),
                (&ViewManifest, VIEW_MANIFEST_KEY, "View manifest"),
            ],
            ReleaseTab::Values | ReleaseTab::Manifest => {
                &[(&ToggleReveal, REVEAL_KEY, "Reveal/Hide")]
            }
            ReleaseTab::Overview | ReleaseTab::Notes | ReleaseTab::Resources => &[],
        };
        for (action, fallback, label) in extra {
            hints = hints.hint(KeyHint::for_action(
                *action,
                Some(PANEL_KEY_CONTEXT),
                fallback,
                *label,
                window,
            ));
        }
        if self.tab == ReleaseTab::Resources {
            hints = hints.hint(
                KeyHint::for_action(
                    &OpenResource,
                    Some(PANEL_KEY_CONTEXT),
                    OPEN_RESOURCE_KEY,
                    "Open",
                    window,
                )
                .essential(),
            );
        }
        hints = hints
            .hint(KeyHint::for_action(
                &RollbackRelease,
                Some(PANEL_KEY_CONTEXT),
                ROLLBACK_KEY,
                "Roll back",
                window,
            ))
            .hint(KeyHint::for_action(
                &UninstallRelease,
                Some(PANEL_KEY_CONTEXT),
                UNINSTALL_KEY,
                "Uninstall",
                window,
            ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .p(space.panel_inset)
            .child(self.tab_bar(window, cx))
            .child(
                div()
                    .id("helm-release-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(crate::ui::style::surface_raised(cx))
                    .child(hints),
            )
            .into_any_element()
    }
}

impl Render for HelmReleasePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let content = self.content(window, cx);
        let header = div()
            .flex()
            .items_center()
            .gap(space.control_gap)
            .px(space.panel_inset)
            .py(space.control_gap)
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(panel_title::context_label(
                &self.scope,
                cx.theme().muted_foreground,
            ))
            .child(div().flex_1())
            .children(self.action_buttons(cx));
        let banner =
            actions::status_banner(&self.actions, cx.weak_entity(), &BANNER, cx).map(|banner| {
                div()
                    .px(space.panel_inset)
                    .pt(space.control_gap)
                    .child(banner)
            });
        div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, _: &ShowOverview, _, cx| this.show(ReleaseTab::Overview, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowHistory, _, cx| this.show(ReleaseTab::History, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowNotes, _, cx| this.show(ReleaseTab::Notes, cx)))
            .on_action(
                cx.listener(|this, _: &RollbackRelease, window, cx| this.rollback(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &UninstallRelease, window, cx| this.uninstall(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowValues, _, cx| this.show(ReleaseTab::Values, cx)))
            .on_action(
                cx.listener(|this, _: &ShowManifest, _, cx| this.show(ReleaseTab::Manifest, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleReveal, _, cx| this.toggle_reveal(cx)))
            .on_action(cx.listener(|this, _: &ViewValues, _, cx| this.view(ReleaseTab::Values, cx)))
            .on_action(
                cx.listener(|this, _: &ViewManifest, _, cx| this.view(ReleaseTab::Manifest, cx)),
            )
            // Copy is handled only while its text is revealed, so its palette
            // entry is disabled otherwise.
            .when(self.revealed_text(ReleaseTab::Values, cx).is_some(), |el| {
                el.on_action(cx.listener(|this, _: &CopyValues, _, cx| {
                    this.copy_revealed(ReleaseTab::Values, cx)
                }))
            })
            .when(
                self.revealed_text(ReleaseTab::Manifest, cx).is_some(),
                |el| {
                    el.on_action(cx.listener(|this, _: &CopyManifest, _, cx| {
                        this.copy_revealed(ReleaseTab::Manifest, cx)
                    }))
                },
            )
            .on_action(
                cx.listener(|this, _: &ShowResources, _, cx| this.show(ReleaseTab::Resources, cx)),
            )
            .on_action(cx.listener(|this, _: &OpenResource, window, cx| {
                if this.tab == ReleaseTab::Resources {
                    this.open_resource(window, cx);
                }
            }))
            // History's cursor: pure row movement, the keyboard rule's one exception.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers.modified() {
                    return;
                }
                let by = match event.keystroke.key.as_str() {
                    "up" => -1,
                    "down" => 1,
                    _ => return,
                };
                match this.tab {
                    ReleaseTab::History => this.move_selection(by, cx),
                    ReleaseTab::Resources => this.move_resource_selection(by, cx),
                    ReleaseTab::Overview
                    | ReleaseTab::Values
                    | ReleaseTab::Manifest
                    | ReleaseTab::Notes => {}
                }
            }))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .children(banner)
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .focus_trap("helm-release-panel-tab-trap", &self.focus_handle)
    }
}

/// `notes` split into the lines it shows, each exactly as written - an empty
/// line kept as a space so it still takes a line's height.
pub(super) fn plain_lines(notes: &str) -> Vec<String> {
    notes
        .lines()
        .map(|line| {
            if line.is_empty() {
                " ".to_string()
            } else {
                line.to_string()
            }
        })
        .collect()
}

/// `text` as plain monospace lines that can be drag-selected across, in order,
/// and copied - the Logs panel's selectable lines (#151) - each line exactly
/// as written, never Markdown.
pub(super) fn selectable_lines(id: &'static str, text: &str) -> Div {
    use crate::ui::typography::TypeRole as _;
    div()
        .flex()
        .flex_col()
        .data_font()
        .children(plain_lines(text).into_iter().enumerate().map(|(ix, line)| {
            div().whitespace_nowrap().child(
                gpui_kit::base::SelectableText::new((id, ix), line).document_order(ix as u64),
            )
        }))
}
