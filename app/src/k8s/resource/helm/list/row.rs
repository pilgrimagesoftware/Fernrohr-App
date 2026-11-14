//! One Helm release as the list shows it: from its latest revision's labels,
//! and - once fetched - what its payload says.

use crate::k8s::resource::helm::{ReleaseRow, Summary};
use jiff::Timestamp;

/// One release row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HelmRow {
    /// `<namespace>/<name>`: what keeps a row selected across updates.
    pub(crate) uid: String,
    pub(crate) name: String,
    pub(crate) namespace: String,
    pub(crate) revision: u32,
    /// As Helm last labelled the latest revision.
    pub(crate) status: String,
    pub(crate) chart: Chart,
    pub(crate) app_version: String,
    pub(crate) updated: Option<Timestamp>,
}

/// The Chart cell: what the latest revision's payload says, while it loads,
/// or that it couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Chart {
    Loading,
    Known(String),
    Unreadable,
}

impl HelmRow {
    pub(crate) fn new(release: &ReleaseRow) -> Self {
        let (chart, app_version, updated) = match &release.summary {
            Summary::Ready { chart, info } => (
                Chart::Known(format!("{}-{}", chart.name, chart.version)),
                chart.app_version.clone().unwrap_or_default(),
                info.last_deployed
                    .as_deref()
                    .and_then(|time| time.parse().ok()),
            ),
            Summary::Loading => (Chart::Loading, String::new(), None),
            Summary::Unreadable(_) => (Chart::Unreadable, String::new(), None),
        };
        Self {
            uid: format!("{}/{}", release.key.namespace, release.key.name),
            name: release.key.name.clone(),
            namespace: release.key.namespace.clone(),
            revision: release.latest.version,
            status: release.latest.status.clone(),
            chart,
            app_version,
            updated,
        }
    }

    /// Whether Helm uninstalled the release but kept its history
    /// (`helm uninstall --keep-history`): hidden unless asked for, as
    /// `helm list` hides it.
    pub(crate) fn uninstalled(&self) -> bool {
        self.status == "uninstalled"
    }

    pub(crate) fn chart_text(&self) -> String {
        match &self.chart {
            Chart::Loading => "\u{2026}".to_string(),
            Chart::Known(chart) => chart.clone(),
            Chart::Unreadable => format!("couldn't read revision {}", self.revision),
        }
    }
}
