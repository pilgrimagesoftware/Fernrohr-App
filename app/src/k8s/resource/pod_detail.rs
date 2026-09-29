//! One Pod's detail panel: a structured field list, or its raw YAML.
//!
//! A dock panel of its own rather than a block inside `PodsPanel`, so detail can
//! be viewed, moved, and closed like any other panel - and so two pods' details
//! can be open side by side. It reads the single `Pod` it was opened for with
//! `Api::get` rather than joining the all-namespaces watch: there is one object,
//! nothing to keep up to date beyond a re-fetch, and no reason to stream a whole
//! cluster's pods into a panel that shows one.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pods::format_age;
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;

/// The label of a field row, and a value shaped so the renderer knows how to
/// draw it without re-deriving that from the string.
///
/// The projection is a plain `&Pod -> Vec<PodField>`, so it is testable as the
/// view model the same way `pod_row` is: a fixture in, asserted rows out, no
/// GPUI harness needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodField {
    pub label: &'static str,
    pub value: PodFieldValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PodFieldValue {
    /// One short value on the row.
    Text(String),
    /// A value naming another object (its namespace, node, owner, service
    /// account). Styled as a link; not clickable yet - see the change's
    /// `design.md` on the deferred navigation pass.
    Link(String),
    /// Key=value pairs, one chip each.
    Chips(Vec<String>),
    /// Conditions, one badge each.
    Badges(Vec<ConditionBadge>),
    /// Rows behind a disclosure, collapsed by default so a long list does not
    /// push the fields the user came for off screen.
    Collapsed(Vec<String>),
}

impl PodFieldValue {
    /// The single-line text this value renders as, for the projection tests
    /// that assert on a value rather than on a rendered element. Composite
    /// values join their parts, so an assertion cannot pass on a value the
    /// user never sees.
    #[cfg(test)]
    pub fn text(&self) -> String {
        match self {
            PodFieldValue::Text(text) | PodFieldValue::Link(text) => text.clone(),
            PodFieldValue::Chips(chips) => chips.join(", "),
            PodFieldValue::Badges(badges) => badges
                .iter()
                .map(|badge| format!("{}={}", badge.condition, badge.status))
                .collect::<Vec<_>>()
                .join(", "),
            PodFieldValue::Collapsed(rows) => rows.join(", "),
        }
    }
}

/// A condition's badge color, decided by the condition rather than looked up at
/// render time - so "True is good, anything else is not" is testable without a
/// theme, and the renderer only maps tone to a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    /// The condition holds.
    Good,
    /// The condition does not hold - a warning to notice, not a failure.
    Warning,
    /// The cluster did not say either way.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionBadge {
    /// The condition's `type` (`Ready`, `PodScheduled`, ...).
    pub condition: String,
    /// Its raw `status` (`True`, `False`, `Unknown`).
    pub status: String,
    pub tone: BadgeTone,
}

fn condition_tone(status: &str) -> BadgeTone {
    match status {
        "True" => BadgeTone::Good,
        "False" => BadgeTone::Warning,
        _ => BadgeTone::Unknown,
    }
}

fn chip(key: &str, value: &str) -> String {
    format!("{key}={value}")
}

/// The fields the panel shows, in the order it shows them. Rows whose source is
/// absent from the pod are left out rather than rendered blank - a pod with no
/// owner references has no "Controlled By" row, it does not have an empty one.
pub fn pod_fields(pod: &Pod, now: Timestamp) -> Vec<PodField> {
    let mut fields = Vec::new();
    let mut push = |label: &'static str, value: PodFieldValue| {
        fields.push(PodField { label, value });
    };

    if let Some(created) = &pod.metadata.creation_timestamp {
        let age_secs = now.duration_since(created.0).as_secs_f64() as i64;
        push(
            "Created",
            PodFieldValue::Text(format!("{} ({})", format_age(age_secs), created.0)),
        );
    }
    if let Some(name) = non_empty(&pod.metadata.name) {
        push("Name", PodFieldValue::Text(name.to_string()));
    }
    if let Some(namespace) = non_empty(&pod.metadata.namespace) {
        push("Namespace", PodFieldValue::Link(namespace.to_string()));
    }
    if let Some(labels) = non_empty_map(&pod.metadata.labels) {
        push(
            "Labels",
            PodFieldValue::Chips(labels.iter().map(|(key, value)| chip(key, value)).collect()),
        );
    }
    if let Some(annotations) = non_empty_map(&pod.metadata.annotations) {
        push(
            "Annotations",
            PodFieldValue::Chips(
                annotations
                    .iter()
                    .map(|(key, value)| chip(key, value))
                    .collect(),
            ),
        );
    }
    let owners: Vec<String> = pod
        .metadata
        .owner_references
        .iter()
        .flatten()
        .map(|owner| format!("{}/{}", owner.kind, owner.name))
        .collect();
    if !owners.is_empty() {
        push("Controlled By", PodFieldValue::Link(owners.join(", ")));
    }
    let managed_fields: Vec<String> = pod
        .metadata
        .managed_fields
        .iter()
        .flatten()
        .map(|entry| match non_empty(&entry.manager) {
            Some(manager) => match non_empty(&entry.operation) {
                Some(operation) => format!("{manager} ({operation})"),
                None => manager.to_string(),
            },
            // A managed-fields entry with no manager still names a manager the
            // API just did not record; say so rather than dropping the row.
            None => entry
                .operation
                .clone()
                .unwrap_or_else(|| "unknown manager".to_string()),
        })
        .collect();
    if !managed_fields.is_empty() {
        push("Managed Fields", PodFieldValue::Collapsed(managed_fields));
    }
    if let Some(phase) = pod
        .status
        .as_ref()
        .and_then(|status| non_empty(&status.phase))
    {
        push("Status", PodFieldValue::Text(phase.to_string()));
    }
    if let Some(node) = pod
        .spec
        .as_ref()
        .and_then(|spec| non_empty(&spec.node_name))
    {
        push("Node", PodFieldValue::Link(node.to_string()));
    }
    if let Some(ips) = ips_of(pod, IpSource::Host) {
        push("Host IPs", PodFieldValue::Text(ips));
    }
    if let Some(ips) = ips_of(pod, IpSource::Pod) {
        push("Pod IPs", PodFieldValue::Text(ips));
    }
    if let Some(account) = pod
        .spec
        .as_ref()
        .and_then(|spec| non_empty(&spec.service_account_name))
    {
        push("Service Account", PodFieldValue::Link(account.to_string()));
    }
    if let Some(qos) = pod
        .status
        .as_ref()
        .and_then(|status| non_empty(&status.qos_class))
    {
        push("QoS Class", PodFieldValue::Text(qos.to_string()));
    }
    if let Some(grace) = pod
        .spec
        .as_ref()
        .and_then(|spec| spec.termination_grace_period_seconds)
    {
        push(
            "Termination Grace Period",
            PodFieldValue::Text(format_age(grace)),
        );
    }
    let tolerations: Vec<String> = pod
        .spec
        .as_ref()
        .into_iter()
        .flat_map(|spec| spec.tolerations.iter().flatten())
        .map(format_toleration)
        .collect();
    if !tolerations.is_empty() {
        push("Tolerations", PodFieldValue::Collapsed(tolerations));
    }
    let conditions: Vec<ConditionBadge> = pod
        .status
        .as_ref()
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| ConditionBadge {
            condition: condition.type_.clone(),
            status: condition.status.clone(),
            tone: condition_tone(&condition.status),
        })
        .collect();
    if !conditions.is_empty() {
        push("Conditions", PodFieldValue::Badges(conditions));
    }

    fields
}

/// One toleration, in kubectl's key/operator/value/effect shape. Absent pieces
/// are left out rather than printed as `None`.
fn format_toleration(toleration: &k8s_openapi::api::core::v1::Toleration) -> String {
    let operator = toleration.operator.as_deref().unwrap_or("Equal");
    let mut parts = Vec::new();
    match (non_empty(&toleration.key), non_empty(&toleration.value)) {
        // `Exists` matches on the key alone, so a value would be noise.
        (Some(key), _) if operator == "Exists" => parts.push(key.to_string()),
        (Some(key), Some(value)) => parts.push(format!("{key}={value}")),
        (Some(key), None) => parts.push(key.to_string()),
        (None, _) => parts.push(format!("<any> ({operator})")),
    }
    if let Some(effect) = non_empty(&toleration.effect) {
        parts.push(effect.to_string());
    }
    if let Some(seconds) = toleration.toleration_seconds {
        parts.push(format!("for {}", format_age(seconds)));
    }
    parts.join(": ")
}

enum IpSource {
    Host,
    Pod,
}

/// The pod's host or pod IPs, joined for display. Falls back to the singular
/// `host_ip`/`pod_ip` field when the plural one is absent, which is how older
/// clusters report a single address.
fn ips_of(pod: &Pod, source: IpSource) -> Option<String> {
    let status = pod.status.as_ref()?;
    let addresses: Vec<String> = match source {
        IpSource::Host => {
            let plural: Vec<String> = status
                .host_ips
                .iter()
                .flatten()
                .map(|ip| ip.ip.clone())
                .collect();
            if plural.is_empty() {
                status.host_ip.iter().cloned().collect()
            } else {
                plural
            }
        }
        IpSource::Pod => {
            let plural: Vec<String> = status
                .pod_ips
                .iter()
                .flatten()
                .map(|ip| ip.ip.clone())
                .collect();
            if plural.is_empty() {
                status.pod_ip.iter().cloned().collect()
            } else {
                plural
            }
        }
    };
    if addresses.is_empty() {
        None
    } else {
        Some(addresses.join(", "))
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.is_empty())
}

fn non_empty_map(
    map: &Option<std::collections::BTreeMap<String, String>>,
) -> Option<&std::collections::BTreeMap<String, String>> {
    map.as_ref().filter(|map| !map.is_empty())
}

/// Which of the panel's two views is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailView {
    /// The structured field list, and the default.
    Structured,
    /// The raw manifest.
    Yaml,
}

/// What the panel knows about the pod it is scoped to.
enum PodDetailState {
    Loading,
    Loaded(Box<Pod>),
    /// The pod is gone. Its own state rather than an error: a detail panel that
    /// outlives its pod is a normal thing to have left open, not a failure.
    NotFound,
    Failed(String),
}

/// One fetch's outcome, so a 404 is told apart from every other error before it
/// reaches the panel's state.
enum PodFetch {
    Found(Box<Pod>),
    NotFound,
}

async fn fetch_pod(
    client: kube::Client,
    namespace: String,
    name: String,
) -> Result<PodFetch, String> {
    let api: Api<Pod> = Api::namespaced(client, &namespace);
    match api.get(&name).await {
        Ok(pod) => Ok(PodFetch::Found(Box::new(pod))),
        Err(kube::Error::Api(status)) if status.code == 404 => Ok(PodFetch::NotFound),
        Err(error) => Err(error.to_string()),
    }
}

pub fn register_restore(cx: &mut gpui_kit::App) {
    register_panel(cx, "PodDetail", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("PodDetail layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("PodDetail layout state must name its cluster")
            .to_string();
        let namespace = state["pod_namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let name = state["pod_name"].as_str().unwrap_or_default().to_string();
        let target = NavTarget::pod(namespace.clone(), name.clone());
        let scope = PanelScope::new(target, context_name);
        panel_handle(cx.new(|cx| PodDetailPanel::new(PodRef { namespace, name }, scope, cx)))
    });
}

/// A dock panel showing one Pod's structured fields, or its raw YAML.
pub struct PodDetailPanel {
    /// Which pod this panel is over - also the identity `PanelKey` dedups on,
    /// through the scope's `NavTarget::Pod`.
    pod: PodRef,
    scope: PanelScope,
    connection: Entity<ClusterConnection>,
    state: PodDetailState,
    viewing: DetailView,
    managed_fields_open: bool,
    tolerations_open: bool,
    /// Whether a fetch is in flight, so a connection that flaps does not race
    /// two results into `state`.
    fetching: bool,
    focus_handle: FocusHandle,
}

impl PodDetailPanel {
    pub fn new(pod: PodRef, scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();

        let mut this = Self {
            pod,
            scope,
            connection,
            state: PodDetailState::Loading,
            viewing: DetailView::Structured,
            managed_fields_open: false,
            tolerations_open: false,
            fetching: false,
            focus_handle: cx.focus_handle(),
        };
        this.sync(cx);
        this
    }

    /// Construction from an explicit connection, so tests can hand in a stub
    /// instead of the real registry - the same seam `ResourcePanel` uses. The
    /// stub stays non-`Connected`, so nothing spawns a real connect task.
    #[cfg(test)]
    fn with_connection(
        pod: PodRef,
        scope: PanelScope,
        connection: Entity<ClusterConnection>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        let mut this = Self {
            pod,
            scope,
            connection,
            state: PodDetailState::Loading,
            viewing: DetailView::Structured,
            managed_fields_open: false,
            tolerations_open: false,
            fetching: false,
            focus_handle: cx.focus_handle(),
        };
        this.sync(cx);
        this
    }

    /// Fetches the pod once its cluster has a client. A no-op while a fetch is
    /// in flight or the connection is not up yet - the observer on the
    /// connection calls this again once it is.
    fn sync(&mut self, cx: &mut Context<Self>) {
        if self.fetching {
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let namespace = self.pod.namespace.clone();
        let name = self.pod.name.clone();
        self.fetching = true;
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(fetch_pod(client, namespace, name).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.fetching = false;
                    this.state = match result {
                        Ok(PodFetch::Found(pod)) => PodDetailState::Loaded(pod),
                        Ok(PodFetch::NotFound) => PodDetailState::NotFound,
                        Err(error) => PodDetailState::Failed(error),
                    };
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    fn set_view(&mut self, view: DetailView, cx: &mut Context<Self>) {
        self.viewing = view;
        cx.notify();
    }

    /// The loaded pod, if the fetch has landed. Read by tests and by render.
    fn pod(&self) -> Option<&Pod> {
        match &self.state {
            PodDetailState::Loaded(pod) => Some(pod),
            _ => None,
        }
    }

    /// The rows the structured view renders, or empty while there is nothing to
    /// show. Projected here rather than in `render` so a test asserts the rows
    /// the panel would draw.
    fn fields(&self, now: Timestamp) -> Vec<PodField> {
        self.pod()
            .map(|pod| pod_fields(pod, now))
            .unwrap_or_default()
    }

    /// The raw manifest, or nothing while there is no pod to render. Shared by
    /// the YAML view and its test, so the test reads exactly what the view
    /// draws rather than a second serialization that could drift from it.
    fn yaml(&self) -> Option<String> {
        self.pod()
            .and_then(|pod| serde_yaml_ng::to_string(pod).ok())
    }

    fn render_field(&self, field: &PodField, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let value =
            match &field.value {
                PodFieldValue::Text(text) => div().child(text.clone()).into_any_element(),
                // Link-styled but not yet clickable - see `design.md` on the
                // deferred navigation pass.
                PodFieldValue::Link(text) => div()
                    .text_color(theme.primary)
                    .child(text.clone())
                    .into_any_element(),
                PodFieldValue::Chips(chips) => div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(chips.iter().map(|chip| {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(theme.muted)
                            .text_sm()
                            .child(chip.clone())
                    }))
                    .into_any_element(),
                PodFieldValue::Badges(badges) => div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(badges.iter().map(|badge| {
                        let color = match badge.tone {
                            BadgeTone::Good => theme.success,
                            BadgeTone::Warning => theme.warning,
                            BadgeTone::Unknown => theme.muted_foreground,
                        };
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(color)
                            .text_color(theme.background)
                            .text_sm()
                            .child(badge.condition.clone())
                    }))
                    .into_any_element(),
                PodFieldValue::Collapsed(rows) => {
                    // Bound before the closure: the id is used twice and the
                    // closure is `'static`, so it cannot borrow `field`.
                    let label = field.label;
                    let open = match label {
                        "Managed Fields" => self.managed_fields_open,
                        _ => self.tolerations_open,
                    };
                    let this = cx.weak_entity();
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Button::new(label)
                                .label(if open { "Hide" } else { "Show" })
                                .xsmall()
                                .ghost()
                                .tab_stop(false)
                                .on_click(move |_event, _window, cx| {
                                    let _ = this.update(cx, |this: &mut Self, cx| {
                                        match label {
                                            "Managed Fields" => {
                                                this.managed_fields_open = !this.managed_fields_open
                                            }
                                            _ => this.tolerations_open = !this.tolerations_open,
                                        }
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(Collapsible::new().open(open).content(
                            div().flex().flex_col().children(
                                rows.iter().map(|row| div().text_sm().child(row.clone())),
                            ),
                        ))
                        .into_any_element()
                }
            };

        div()
            .flex()
            .gap_3()
            .py_1()
            .child(
                div()
                    .w(px(180.))
                    .flex_none()
                    .text_color(theme.muted_foreground)
                    .child(field.label),
            )
            .child(div().flex_1().child(value))
            .into_any_element()
    }

    fn render_structured(&self, cx: &Context<Self>) -> AnyElement {
        let fields = self.fields(Timestamp::now());
        div()
            .flex()
            .flex_col()
            .children(fields.iter().map(|field| self.render_field(field, cx)))
            .into_any_element()
    }

    fn render_yaml(&self, cx: &App) -> AnyElement {
        let Some(yaml) = self.yaml() else {
            return div().into_any_element();
        };
        div()
            .size_full()
            .font_family(cx.theme().mono_font_family.clone())
            .whitespace_nowrap()
            .child(yaml)
            .into_any_element()
    }
}

impl Focusable for PodDetailPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PodDetailPanel {}

impl Render for PodDetailPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.state {
            PodDetailState::Loading => div()
                .size_full()
                .p_3()
                .child("Loading pod...")
                .into_any_element(),
            PodDetailState::NotFound => div()
                .size_full()
                .p_3()
                .child("This pod no longer exists.")
                .into_any_element(),
            PodDetailState::Failed(reason) => div()
                .size_full()
                .p_3()
                .child(format!("Could not read pod: {reason}"))
                .into_any_element(),
            PodDetailState::Loaded(_) => {
                let inner = match self.viewing {
                    DetailView::Structured => self.render_structured(cx),
                    DetailView::Yaml => self.render_yaml(cx),
                };
                div()
                    .size_full()
                    .p_3()
                    .overflow_scrollbar()
                    .child(inner)
                    .into_any_element()
            }
        };

        panel_title::focus_frame(body, &self.focus_handle, window, cx)
    }
}

impl BasePanel for PodDetailPanel {
    fn panel_name(&self) -> &'static str {
        "PodDetail"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "pod_namespace": self.pod.namespace,
                "pod_name": self.pod.name,
            })),
        }
    }
}

/// The title bar, supplied to the dock rather than drawn here - the same bar
/// every other panel gets, so `"Pod: <name>"` comes from section 2's rule
/// rather than a second implementation in this panel.
///
/// No namespace picker: the panel is over one pod, not a namespace-scoped list,
/// so there is no scope for a picker to change.
impl Panel for PodDetailPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title(&self.scope)
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        let yaml = self.viewing == DetailView::Yaml;
        let this = cx.weak_entity();
        let mut buttons = vec![
            Button::new("pod-detail-view")
                .label(if yaml { "Fields" } else { "YAML" })
                .icon(if yaml { IconName::List } else { IconName::Code })
                .xsmall()
                .ghost()
                .tab_stop(false)
                .toggled(yaml)
                .tooltip("Switch between the field list and raw YAML")
                .on_click(move |_event, _window, cx| {
                    let _ = this.update(cx, |this: &mut Self, cx| {
                        let next = match this.viewing {
                            DetailView::Structured => DetailView::Yaml,
                            DetailView::Yaml => DetailView::Structured,
                        };
                        this.set_view(next, cx);
                    });
                }),
        ];
        buttons.extend(panel_title::toolbar_buttons().unwrap_or_default());
        Some(buttons)
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: `gpui_kit::*` re-exports its own `test` macro, which
    // would shadow `core::prelude::v1::test` for these plain synchronous tests.
    use super::{
        BadgeTone, DetailView, PodDetailPanel, PodDetailState, PodFetch, PodField, PodFieldValue,
        fetch_pod, pod_fields,
    };
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::ui::nav::{NavTarget, PodRef};
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::{AppContext as _, TestAppContext};
    use jiff::Timestamp;
    use k8s_openapi::api::core::v1::{
        Container, HostIP, Pod, PodCondition, PodIP, PodSpec, PodStatus, Toleration,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
        ManagedFieldsEntry, ObjectMeta, OwnerReference, Time,
    };

    fn field<'a>(fields: &'a [PodField], label: &str) -> Option<&'a PodField> {
        fields.iter().find(|field| field.label == label)
    }

    /// A pod carrying every field the projection reads, so a test can assert
    /// the whole list rather than each field in isolation.
    fn rich_pod() -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some("api-7d9f-ftg5t".into()),
                namespace: Some("staging".into()),
                creation_timestamp: Some(Time(Timestamp::from_second(0).unwrap())),
                labels: Some(
                    [
                        ("app".to_string(), "api".to_string()),
                        ("tier".to_string(), "backend".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                annotations: Some(
                    [
                        ("team".to_string(), "platform".to_string()),
                        ("checked".to_string(), "yes".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                owner_references: Some(vec![OwnerReference {
                    api_version: "apps/v1".into(),
                    kind: "ReplicaSet".into(),
                    name: "api-7d9f".into(),
                    uid: "owner-1".into(),
                    controller: Some(true),
                    ..Default::default()
                }]),
                managed_fields: Some(vec![
                    ManagedFieldsEntry {
                        manager: Some("kubelet".into()),
                        operation: Some("Update".into()),
                        ..Default::default()
                    },
                    ManagedFieldsEntry {
                        manager: Some("kube-controller-manager".into()),
                        operation: Some("Update".into()),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            },
            spec: Some(PodSpec {
                node_name: Some("node-a".into()),
                service_account_name: Some("api".into()),
                termination_grace_period_seconds: Some(30),
                tolerations: Some(vec![Toleration {
                    key: Some("dedicated".into()),
                    operator: Some("Equal".into()),
                    value: Some("api".into()),
                    effect: Some("NoSchedule".into()),
                    ..Default::default()
                }]),
                containers: vec![Container {
                    name: "app".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some("Running".into()),
                qos_class: Some("Burstable".into()),
                host_ips: Some(vec![HostIP {
                    ip: "10.0.0.1".into(),
                }]),
                pod_ips: Some(vec![PodIP {
                    ip: "10.1.0.7".into(),
                }]),
                conditions: Some(vec![
                    PodCondition {
                        type_: "Ready".into(),
                        status: "True".into(),
                        ..Default::default()
                    },
                    PodCondition {
                        type_: "PodScheduled".into(),
                        status: "False".into(),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
        }
    }

    /// Section 3.1: every field the design lists is present, in order, and a
    /// pod with more than one label/annotation/toleration/condition keeps them
    /// all as discrete items rather than collapsing them into one string.
    #[test]
    fn every_structured_field_is_projected_in_order() {
        let fields = pod_fields(&rich_pod(), Timestamp::from_second(90).unwrap());

        let labels: Vec<&str> = fields.iter().map(|f| f.label).collect();
        assert_eq!(
            labels,
            vec![
                "Created",
                "Name",
                "Namespace",
                "Labels",
                "Annotations",
                "Controlled By",
                "Managed Fields",
                "Status",
                "Node",
                "Host IPs",
                "Pod IPs",
                "Service Account",
                "QoS Class",
                "Termination Grace Period",
                "Tolerations",
                "Conditions",
            ]
        );

        assert_eq!(
            field(&fields, "Created").unwrap().value.text(),
            "1m (1970-01-01T00:00:00Z)"
        );
        assert_eq!(
            field(&fields, "Name").unwrap().value.text(),
            "api-7d9f-ftg5t"
        );
        assert_eq!(field(&fields, "Namespace").unwrap().value.text(), "staging");
        assert_eq!(
            field(&fields, "Labels").unwrap().value,
            PodFieldValue::Chips(vec!["app=api".into(), "tier=backend".into()]),
            "one chip per label"
        );
        assert_eq!(
            field(&fields, "Annotations").unwrap().value,
            PodFieldValue::Chips(vec!["checked=yes".into(), "team=platform".into()])
        );
        assert_eq!(
            field(&fields, "Controlled By").unwrap().value,
            PodFieldValue::Link("ReplicaSet/api-7d9f".into())
        );
        assert_eq!(field(&fields, "Status").unwrap().value.text(), "Running");
        assert_eq!(
            field(&fields, "Managed Fields").unwrap().value,
            PodFieldValue::Collapsed(vec![
                "kubelet (Update)".into(),
                "kube-controller-manager (Update)".into(),
            ]),
            "one row per manager, behind a disclosure"
        );
        assert_eq!(field(&fields, "Node").unwrap().value.text(), "node-a");
        assert_eq!(field(&fields, "Host IPs").unwrap().value.text(), "10.0.0.1");
        assert_eq!(field(&fields, "Pod IPs").unwrap().value.text(), "10.1.0.7");
        assert_eq!(
            field(&fields, "Service Account").unwrap().value.text(),
            "api"
        );
        assert_eq!(
            field(&fields, "QoS Class").unwrap().value.text(),
            "Burstable"
        );
        assert_eq!(
            field(&fields, "Termination Grace Period")
                .unwrap()
                .value
                .text(),
            "30s"
        );
        assert_eq!(
            field(&fields, "Tolerations").unwrap().value,
            PodFieldValue::Collapsed(vec!["dedicated=api: NoSchedule".into()])
        );
    }

    /// Section 3.1: a condition's badge tone follows its own status, so the
    /// renderer cannot color a `False` condition green.
    #[test]
    fn condition_badges_tone_by_their_status() {
        let fields = pod_fields(&rich_pod(), Timestamp::from_second(0).unwrap());
        let PodFieldValue::Badges(badges) = &field(&fields, "Conditions").unwrap().value else {
            panic!("conditions render as badges");
        };

        assert_eq!(badges.len(), 2, "one badge per condition");
        assert_eq!(badges[0].condition, "Ready");
        assert_eq!(badges[0].status, "True");
        assert_eq!(badges[0].tone, BadgeTone::Good);
        assert_eq!(badges[1].condition, "PodScheduled");
        assert_eq!(badges[1].status, "False");
        assert_eq!(
            badges[1].tone,
            BadgeTone::Warning,
            "a condition that does not hold is not green"
        );
    }

    /// A condition the cluster did not resolve reads as neither good nor bad.
    #[test]
    fn an_unresolved_condition_is_neither_good_nor_a_warning() {
        let mut pod = rich_pod();
        pod.status.as_mut().unwrap().conditions = Some(vec![PodCondition {
            type_: "Ready".into(),
            status: "Unknown".into(),
            ..Default::default()
        }]);

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        let PodFieldValue::Badges(badges) = &field(&fields, "Conditions").unwrap().value else {
            panic!("conditions render as badges");
        };
        assert_eq!(badges[0].tone, BadgeTone::Unknown);
    }

    /// Section 3.2: rows whose source is absent are left out, not shown blank -
    /// so a minimal pod reads as a short list, not a page of empty values.
    #[test]
    fn absent_fields_are_omitted_rather_than_blank() {
        let minimal = Pod {
            metadata: ObjectMeta {
                name: Some("bare".into()),
                namespace: Some("default".into()),
                ..Default::default()
            },
            ..Default::default()
        };

        let fields = pod_fields(&minimal, Timestamp::from_second(0).unwrap());
        for absent in [
            "Created",
            "Labels",
            "Annotations",
            "Controlled By",
            "Managed Fields",
            "Status",
            "Node",
            "Host IPs",
            "Pod IPs",
            "Service Account",
            "QoS Class",
            "Termination Grace Period",
            "Tolerations",
            "Conditions",
        ] {
            assert!(
                field(&fields, absent).is_none(),
                "{absent} has no source on a pod without one"
            );
        }
        assert_eq!(
            fields.iter().map(|f| f.label).collect::<Vec<_>>(),
            vec!["Name", "Namespace"],
            "only the fields the pod actually has"
        );
    }

    /// Section 3.2's "a single IP" case: a cluster reporting one address in the
    /// singular field still gets a row, rather than no row at all.
    #[test]
    fn a_single_ip_reported_singularly_still_renders() {
        let mut pod = rich_pod();
        let status = pod.status.as_mut().unwrap();
        status.host_ips = None;
        status.host_ip = Some("192.168.1.5".into());
        status.pod_ips = None;
        status.pod_ip = Some("192.168.1.6".into());

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        assert_eq!(
            field(&fields, "Host IPs").unwrap().value.text(),
            "192.168.1.5"
        );
        assert_eq!(
            field(&fields, "Pod IPs").unwrap().value.text(),
            "192.168.1.6"
        );
    }

    /// An `Exists` toleration matches on its key alone, so a value it does not
    /// have must not be invented for it.
    #[test]
    fn an_exists_toleration_reads_by_its_key_alone() {
        let mut pod = rich_pod();
        pod.spec.as_mut().unwrap().tolerations = Some(vec![Toleration {
            key: Some("node-role.kubernetes.io/control-plane".into()),
            operator: Some("Exists".into()),
            effect: Some("NoSchedule".into()),
            ..Default::default()
        }]);

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        assert_eq!(
            field(&fields, "Tolerations").unwrap().value,
            PodFieldValue::Collapsed(vec![
                "node-role.kubernetes.io/control-plane: NoSchedule".into()
            ])
        );
    }

    fn stub_panel(
        cx: &mut TestAppContext,
        state: ConnectionState,
    ) -> gpui_kit::WindowHandle<PodDetailPanel> {
        let connection = cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(state)));
        cx.add_window(|_window, cx| {
            let pod = PodRef {
                namespace: "staging".into(),
                name: "api-7d9f-ftg5t".into(),
            };
            let scope = PanelScope::new(
                NavTarget::pod("staging", "api-7d9f-ftg5t"),
                "kind-dev".into(),
            );
            PodDetailPanel::with_connection(pod, scope, connection, cx)
        })
    }

    /// Section 4.1: the structured field list is the panel's default view, and
    /// it is the projection of the pod it holds - not a text blob.
    #[gpui_kit::test]
    async fn the_panel_renders_the_structured_field_list_by_default(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()));
                cx.notify();
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.viewing, DetailView::Structured);
                let fields = panel.fields(Timestamp::from_second(90).unwrap());
                assert!(field(&fields, "Name").is_some());
                assert!(field(&fields, "Conditions").is_some());
                assert!(
                    matches!(
                        field(&fields, "Labels").unwrap().value,
                        PodFieldValue::Chips(_)
                    ),
                    "labels are chips, not one run-on line"
                );
                assert!(
                    matches!(
                        field(&fields, "Conditions").unwrap().value,
                        PodFieldValue::Badges(_)
                    ),
                    "conditions are badges"
                );
                assert!(
                    matches!(
                        field(&fields, "Managed Fields").unwrap().value,
                        PodFieldValue::Collapsed(_)
                    ),
                    "managed fields start collapsed"
                );
            })
            .unwrap();
        // The render itself runs as part of the window's frame, so a panic in
        // it would fail here rather than being silently skipped.
        cx.run_until_parked();
    }

    /// Section 4.3: the toolbar toggle swaps the field list for the raw YAML
    /// and back, without leaving the panel.
    #[gpui_kit::test]
    async fn the_toolbar_toggles_between_fields_and_yaml(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()));
                panel.set_view(DetailView::Yaml, cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.viewing, DetailView::Yaml);
                let yaml = panel.yaml().expect("a loaded pod renders as YAML");
                assert!(yaml.contains("api-7d9f-ftg5t"), "the manifest is the pod's");
            })
            .unwrap();
    }

    /// Section 4.4: a pod that is gone is reported as gone - its own state, not
    /// an error, and not a closed panel.
    #[gpui_kit::test]
    async fn a_missing_pod_shows_that_it_no_longer_exists(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::NotFound;
                cx.notify();
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert!(matches!(panel.state, PodDetailState::NotFound));
                assert!(panel.pod().is_none());
                assert!(panel.fields(Timestamp::now()).is_empty());
            })
            .unwrap();
        cx.run_until_parked();
    }

    /// The fetch tells a 404 apart from every other failure, against a real
    /// (fixture) client - one that serves a pod, and 404s for anything else.
    #[test]
    fn fetch_distinguishes_a_missing_pod_from_other_failures() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (addr, _server) = runtime.block_on(spawn_fixture_api_server());

        runtime.block_on(async {
            let client = kube::Client::try_from(kube::Config::new(
                format!("http://{addr}").parse().unwrap(),
            ))
            .expect("build client");

            let found = fetch_pod(client.clone(), "staging".into(), "present".into())
                .await
                .expect("a 200 is not a failure");
            match found {
                PodFetch::Found(pod) => assert_eq!(pod.metadata.name.as_deref(), Some("present")),
                PodFetch::NotFound => panic!("the server serves this pod"),
            }

            let missing = fetch_pod(client, "staging".into(), "gone".into())
                .await
                .expect("a 404 is a state, not a failure");
            assert!(matches!(missing, PodFetch::NotFound));
        });
    }

    /// A fixture API server serving one pod by name and 404ing every other
    /// name - the smallest thing that exercises `fetch_pod`'s error split.
    async fn spawn_fixture_api_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 4096];
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let (status, body) = if request.contains("/pods/present") {
                        let pod = serde_json::json!({
                            "apiVersion": "v1",
                            "kind": "Pod",
                            "metadata": { "name": "present", "namespace": "staging" },
                        });
                        ("200 OK", pod.to_string())
                    } else {
                        let status = serde_json::json!({
                            "kind": "Status",
                            "apiVersion": "v1",
                            "status": "Failure",
                            "message": "pods \"gone\" not found",
                            "reason": "NotFound",
                            "code": 404,
                        });
                        ("404 Not Found", status.to_string())
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });
        (addr, handle)
    }
}
