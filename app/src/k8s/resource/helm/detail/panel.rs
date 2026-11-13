//! The release detail panel's state: which release, the release watch it
//! follows, the revisions it has fetched, and its tab and History selection.
//! What it draws is `render`'s.

use super::fetch::{Fetched, RevisionDetail, fetch_revision};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::helm::{HelmReleasesTable, HelmScope, ReleaseKey, Revision};
use crate::k8s::resource::secret_value::SecretValue;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState,
};
use gpui_kit::*;
use std::collections::HashMap;

/// The panel's tabs, in order (helm-releases D5). Values, Manifest and
/// Resources join with their sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ReleaseTab {
    #[default]
    Overview,
    History,
    Notes,
}

impl ReleaseTab {
    pub(crate) const ALL: [ReleaseTab; 3] =
        [ReleaseTab::Overview, ReleaseTab::History, ReleaseTab::Notes];

    pub(crate) fn id(self) -> &'static str {
        match self {
            ReleaseTab::Overview => "overview",
            ReleaseTab::History => "history",
            ReleaseTab::Notes => "notes",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            ReleaseTab::Overview => "Overview",
            ReleaseTab::History => "History",
            ReleaseTab::Notes => "Notes",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.id() == id)
    }
}

/// A dock panel over one Helm release.
pub struct HelmReleasePanel {
    pub(super) scope: PanelScope,
    pub(crate) key: ReleaseKey,
    pub(super) connection: Entity<ClusterConnection>,
    /// The release watch over the release's namespace, once connected.
    pub(super) table: Option<Entity<HelmReleasesTable>>,
    /// Each fetched revision: the resourceVersion it was fetched at, and what
    /// it said.
    pub(super) revisions: HashMap<u32, (String, Fetched)>,
    /// The latest revision's notes, and which revision they are.
    pub(super) notes: Option<(u32, SecretValue)>,
    pub(crate) tab: ReleaseTab,
    /// The revision selected in History; `None` follows the deployed one.
    pub(super) selected: Option<u32>,
    pub(super) focus_handle: FocusHandle,
}

impl HelmReleasePanel {
    pub fn new(key: ReleaseKey, scope: PanelScope, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| {
            this.subscribe(cx);
            cx.notify();
        })
        .detach();
        cx.on_release(|this: &mut Self, cx| {
            if this.table.take().is_some() {
                let scope = HelmScope::Namespace(this.key.namespace.clone());
                ClusterRegistry::unsubscribe_helm_releases(cx, &this.scope.context_name, &scope);
            }
        })
        .detach();
        let mut this = Self {
            scope,
            key,
            connection,
            table: None,
            revisions: HashMap::new(),
            notes: None,
            tab: ReleaseTab::default(),
            selected: None,
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        };
        this.subscribe(cx);
        this
    }

    fn client(&self, cx: &App) -> Option<kube::Client> {
        match &self.connection.read(cx).state {
            ConnectionState::Connected(client) => Some(client.clone()),
            _ => None,
        }
    }

    fn subscribe(&mut self, cx: &mut Context<Self>) {
        if self.table.is_some() {
            return;
        }
        let Some(client) = self.client(cx) else {
            return;
        };
        let scope = HelmScope::Namespace(self.key.namespace.clone());
        let table =
            ClusterRegistry::subscribe_helm_releases(cx, &self.scope.context_name, client, scope);
        cx.observe(&table, |this: &mut Self, _, cx| {
            this.fetch_needed(cx);
            cx.notify();
        })
        .detach();
        self.table = Some(table);
        self.fetch_needed(cx);
    }

    /// The release's revisions, newest first, from its Secrets' labels.
    pub(crate) fn history(&self, cx: &App) -> Vec<Revision> {
        self.table
            .as_ref()
            .map(|table| table.read(cx).history(&self.key))
            .unwrap_or_default()
    }

    /// Whether the release is gone: its watch has listed, and no revision of
    /// it is left.
    pub(crate) fn gone(&self, cx: &App) -> bool {
        self.table
            .as_ref()
            .is_some_and(|table| table.read(cx).loaded())
            && self.history(cx).is_empty()
    }

    /// What revision `version` said, once fetched.
    pub(crate) fn detail(&self, version: u32) -> Option<&RevisionDetail> {
        match self.revisions.get(&version) {
            Some((_, Fetched::Ready(detail))) => Some(detail),
            _ => None,
        }
    }

    /// Where revision `version`'s fetch is.
    pub(crate) fn fetched(&self, version: u32) -> Option<&Fetched> {
        self.revisions.get(&version).map(|(_, fetched)| fetched)
    }

    /// The latest revision's notes as text, once fetched.
    pub(crate) fn notes(&self, cx: &App) -> Option<&str> {
        let latest = self.history(cx).first()?.version;
        match &self.notes {
            Some((version, notes)) if *version == latest => notes.expose(),
            _ => None,
        }
    }

    /// The revision History's selection is on: the chosen one, else the
    /// deployed one, else the latest.
    pub(crate) fn selected_revision(&self, cx: &App) -> Option<u32> {
        let history = self.history(cx);
        self.selected
            .filter(|version| history.iter().any(|revision| revision.version == *version))
            .or_else(|| {
                history
                    .iter()
                    .find(|revision| revision.status == "deployed")
                    .or(history.first())
                    .map(|revision| revision.version)
            })
    }

    /// Moves History's selection `by` rows (negative is up, toward newer).
    pub(super) fn move_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let history = self.history(cx);
        let Some(current) = self.selected_revision(cx) else {
            return;
        };
        let index = history
            .iter()
            .position(|revision| revision.version == current)
            .unwrap_or(0) as isize;
        let next = (index + by).clamp(0, history.len() as isize - 1) as usize;
        self.selected = Some(history[next].version);
        cx.notify();
    }

    pub(super) fn show(&mut self, tab: ReleaseTab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.fetch_needed(cx);
        cx.notify();
    }

    /// Starts the fetches the panel needs now: the latest revision always,
    /// and every other one once History is shown - each again only when its
    /// Secret changed.
    pub(super) fn fetch_needed(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client(cx) else {
            return;
        };
        // Not while the first list arrives, when each revision is briefly the
        // latest.
        if !self
            .table
            .as_ref()
            .is_some_and(|table| table.read(cx).loaded())
        {
            return;
        }
        let history = self.history(cx);
        let wanted: Vec<&Revision> = match self.tab {
            ReleaseTab::History => history.iter().collect(),
            ReleaseTab::Overview | ReleaseTab::Notes => history.iter().take(1).collect(),
        };
        let latest = history.first().map(|revision| revision.version);
        for revision in wanted {
            let current = self
                .revisions
                .get(&revision.version)
                .is_some_and(|(rv, _)| *rv == revision.resource_version);
            if current {
                continue;
            }
            self.revisions.insert(
                revision.version,
                (revision.resource_version.clone(), Fetched::Loading),
            );
            let (version, rv) = (revision.version, revision.resource_version.clone());
            let is_latest = latest == Some(version);
            fetch_revision(
                client.clone(),
                self.key.namespace.clone(),
                revision.secret.clone(),
                cx.weak_entity(),
                move |this: &mut Self, decoded, cx| {
                    let still = this
                        .revisions
                        .get(&version)
                        .is_some_and(|(fetched_at, _)| *fetched_at == rv);
                    if !still {
                        return;
                    }
                    let fetched = match decoded {
                        Ok((detail, notes)) => {
                            if is_latest {
                                this.notes = Some((version, notes));
                            }
                            Fetched::Ready(detail)
                        }
                        Err(reason) => Fetched::Unreadable(reason),
                    };
                    this.revisions.insert(version, (rv, fetched));
                    cx.notify();
                },
                cx,
            );
        }
    }
}

impl Focusable for HelmReleasePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for HelmReleasePanel {}
impl EventEmitter<ScopeEvent> for HelmReleasePanel {}

impl BasePanel for HelmReleasePanel {
    fn panel_name(&self) -> &'static str {
        "HelmRelease"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(super::restore::dump(self)),
        }
    }
}

impl Panel for HelmReleasePanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            cx.entity_id(),
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

crate::scoped_panel!(HelmReleasePanel);
