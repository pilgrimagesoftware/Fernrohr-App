//! The object panel: its state, its lifecycle (fetch once connected), its
//! action handlers and its dock integration. What it draws is `render`'s.

use super::commands::ToggleObjectView;
use super::fetch::{ObjectDetailState, ObjectFetch, fetch_object};
use super::live::LiveSource;
use super::model::{ObjectSection, go_to_entries};
use super::{metadata, restore, sections};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::{DiscoveredKinds, DiscoveryRegistry};
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::link::{self, GoToEntry, GoToReference};
use crate::ui::nav::ObjectTarget;
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState,
};
use gpui_kit::*;
use jiff::Timestamp;
use kube::api::DynamicObject;

/// A dock panel showing one object's structured fields, or its YAML.
pub struct ObjectDetailPanel {
    /// Which object - also the identity `PanelKey` dedups on, through the
    /// scope's `NavTarget::Object`.
    pub(super) target: ObjectTarget,
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<ClusterConnection>,
    /// The context's discovered kinds, which decide which references are
    /// links. Observed, so references turn into links when discovery lands.
    pub(super) discovery: Entity<DiscoveredKinds>,
    pub(super) state: ObjectDetailState,
    pub(super) viewing: DetailView,
    /// Whether a fetch is in flight, so a flapping connection can't race two
    /// results into `state`.
    fetching: bool,
    /// The table changed while a fetch was in flight, so fetch again once it
    /// lands - the in-flight one may predate the change.
    refetch: bool,
    /// The kind's shared watch the panel follows its object in (`live`).
    pub(super) live: super::live::LiveSource,
    /// The write of the object the panel last acted on from the table.
    pub(super) followed: Option<super::live::Version>,
    /// That the object was deleted (its last state kept, stale) or replaced
    /// by a new one of the same name.
    pub(super) lifecycle: Option<crate::ui::detail::lifecycle::Lifecycle>,
    /// The YAML edit in progress (`edit`), if any.
    pub(super) edit: Option<super::edit::YamlEdit>,
    /// Why Edit did nothing - a Secret, say - until the next edit.
    pub(super) edit_notice: Option<String>,
    /// Revealed Secret values, by key - only while shown, and never saved.
    pub(super) revealed:
        std::collections::HashMap<String, crate::k8s::resource::secret_value::Reveal>,
    /// The YAML view's folds and scroll position.
    pub(super) yaml_view: crate::ui::yaml_view::YamlViewState,
    pub(super) focus_handle: FocusHandle,
}

impl ObjectDetailPanel {
    pub fn new(target: ObjectTarget, scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        let discovery = DiscoveryRegistry::kinds(cx, &scope.context_name);
        Self::build(
            target,
            scope,
            connection,
            discovery,
            LiveSource::Pending,
            cx,
        )
    }

    /// Construction from an explicit connection and discovery result, so tests
    /// never start a real connect. The connection stays non-`Connected`.
    #[cfg(test)]
    pub(super) fn with_connection(
        target: ObjectTarget,
        scope: PanelScope,
        connection: Entity<ClusterConnection>,
        kinds: Vec<DiscoveredKind>,
        cx: &mut Context<Self>,
    ) -> Self {
        let discovery = cx.new(|_| DiscoveredKinds::loaded(kinds));
        Self::build(target, scope, connection, discovery, LiveSource::Off, cx)
    }

    fn build(
        target: ObjectTarget,
        scope: PanelScope,
        connection: Entity<ClusterConnection>,
        discovery: Entity<DiscoveredKinds>,
        live: LiveSource,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        cx.observe(&discovery, |_, _, cx| cx.notify()).detach();
        let mut this = Self {
            target,
            scope,
            connection,
            discovery,
            state: ObjectDetailState::Loading,
            viewing: DetailView::Structured,
            fetching: false,
            refetch: false,
            live,
            followed: None,
            lifecycle: None,
            edit: None,
            edit_notice: None,
            revealed: Default::default(),
            yaml_view: Default::default(),
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        };
        this.sync(cx);
        this
    }

    /// Once its cluster has a client, subscribes to the kind's shared watch the
    /// panel follows its object in (`live`), and fetches the object. A no-op
    /// while the connection isn't up - the connection observer calls this
    /// again once it is.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        self.follow_kind(client, cx);
        if !self.fetching {
            self.fetch(cx);
        }
    }

    /// Fetches the object. One in flight already is followed by another once it
    /// lands, so the result reflects the latest change asked for.
    pub(super) fn fetch(&mut self, cx: &mut Context<Self>) {
        if self.fetching {
            self.refetch = true;
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let target = self.target.clone();
        self.fetching = true;
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(fetch_object(client, target).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.fetching = false;
                    match result {
                        Ok(ObjectFetch::Found(object, events)) => this.show_fetched(object, events),
                        Ok(ObjectFetch::NotFound) => {
                            this.show_absent();
                        }
                        Err((message, detail)) => {
                            this.state = ObjectDetailState::Failed { message, detail };
                        }
                    }
                    if std::mem::take(&mut this.refetch) {
                        this.fetch(cx);
                    }
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// Lands `object` as if a fetch had returned it (after redaction, as the
    /// real fetch does), for tests.
    #[cfg(test)]
    pub(crate) fn test_set_loaded(&mut self, mut object: DynamicObject, cx: &mut Context<Self>) {
        super::redact::redact(&self.target.kind, &mut object);
        self.state = ObjectDetailState::Loaded(Box::new(object), Ok(Vec::new()));
        cx.notify();
    }

    /// Lands the not-found state, for tests.
    #[cfg(test)]
    pub(crate) fn test_set_not_found(&mut self, cx: &mut Context<Self>) {
        self.state = ObjectDetailState::NotFound;
        cx.notify();
    }

    /// The loaded object, if the fetch has landed.
    pub(super) fn object(&self) -> Option<&DynamicObject> {
        match &self.state {
            ObjectDetailState::Loaded(object, _) => Some(object),
            _ => None,
        }
    }

    /// Overview, then the kind's own sections - or nothing before the object
    /// has loaded. Projected here rather than in `render` so a test asserts the
    /// rows the panel would draw.
    pub(super) fn sections(&self, now: Timestamp) -> Vec<ObjectSection> {
        let Some(object) = self.object() else {
            return Vec::new();
        };
        let mut sections = vec![metadata::overview(object, &self.target, now)];
        sections.extend(sections::sections_for(&self.target.kind, object));
        sections
    }

    /// The (already redacted) manifest as YAML, or nothing before it loads.
    pub(super) fn yaml(&self) -> Option<String> {
        self.object()
            .and_then(|object| serde_yaml_ng::to_string(object).ok())
    }

    /// The discovered kinds references are resolved against.
    pub(super) fn kinds<'a>(&self, cx: &'a App) -> Option<&'a [DiscoveredKind]> {
        self.discovery.read(cx).kinds()
    }

    /// What the "Go to…" picker offers: every followable reference shown.
    pub(super) fn followable(&self, cx: &App) -> Vec<GoToEntry> {
        link::followable(
            go_to_entries(&self.sections(Timestamp::now())),
            self.kinds(cx),
        )
    }

    /// Shows `view`: the window sets it on open, for a request that named one.
    pub(crate) fn set_view(&mut self, view: DetailView, cx: &mut Context<Self>) {
        self.viewing = view;
        cx.notify();
    }

    /// Which view is showing. Test-only.
    #[cfg(test)]
    pub(crate) fn view(&self) -> DetailView {
        self.viewing
    }

    pub(super) fn on_action_toggle_view(
        &mut self,
        _: &ToggleObjectView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = match self.viewing {
            DetailView::Structured => DetailView::Yaml,
            DetailView::Yaml => DetailView::Structured,
        };
        self.set_view(next, cx);
    }

    pub(super) fn on_action_go_to(
        &mut self,
        _: &GoToReference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.followable(cx);
        if entries.is_empty() {
            return;
        }
        link::open_go_to(
            entries,
            self.scope.context_name.clone(),
            self.focus_handle.clone(),
            window,
            cx,
        );
    }
}

impl Focusable for ObjectDetailPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for ObjectDetailPanel {}

impl BasePanel for ObjectDetailPanel {
    fn panel_name(&self) -> &'static str {
        "ObjectDetail"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(restore::dump_target(&self.target, &self.scope.context_name)),
        }
    }
}

/// The dock's title bar, from the same rules every panel's comes from. No
/// namespace picker: the panel is over one object.
impl Panel for ObjectDetailPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            window,
            cx,
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}
