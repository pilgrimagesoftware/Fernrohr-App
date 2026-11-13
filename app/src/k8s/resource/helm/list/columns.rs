//! The Helm Releases list's columns: identity, title, default width, the text
//! a cell shows, and how two rows compare on it.

use super::row::HelmRow;
use crate::k8s::resource::pods::format_age;
use crate::ui::style::Tone;
use jiff::Timestamp;
use std::cmp::Ordering;

/// One column. A closed enum, so a new column fails to compile wherever it
/// isn't handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelmColumn {
    Name,
    Namespace,
    Revision,
    Status,
    Chart,
    AppVersion,
    Updated,
}

impl HelmColumn {
    pub(crate) const DEFAULT_ORDER: [HelmColumn; 7] = [
        HelmColumn::Name,
        HelmColumn::Namespace,
        HelmColumn::Revision,
        HelmColumn::Status,
        HelmColumn::Chart,
        HelmColumn::AppVersion,
        HelmColumn::Updated,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            HelmColumn::Name => "name",
            HelmColumn::Namespace => "namespace",
            HelmColumn::Revision => "revision",
            HelmColumn::Status => "status",
            HelmColumn::Chart => "chart",
            HelmColumn::AppVersion => "app_version",
            HelmColumn::Updated => "updated",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            HelmColumn::Name => "Name",
            HelmColumn::Namespace => "Namespace",
            HelmColumn::Revision => "Revision",
            HelmColumn::Status => "Status",
            HelmColumn::Chart => "Chart",
            HelmColumn::AppVersion => "App Version",
            HelmColumn::Updated => "Updated",
        }
    }

    pub(crate) fn default_width(self) -> f32 {
        match self {
            HelmColumn::Name => 200.,
            HelmColumn::Namespace => 140.,
            HelmColumn::Revision => 80.,
            HelmColumn::Status => 150.,
            HelmColumn::Chart => 220.,
            HelmColumn::AppVersion => 110.,
            HelmColumn::Updated => 90.,
        }
    }

    /// The column an id names - a saved sort may name one this build lacks.
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::DEFAULT_ORDER.into_iter().find(|col| col.id() == id)
    }

    /// The text `row` shows here, its age read against `now`.
    pub(crate) fn text(self, row: &HelmRow, now: Timestamp) -> String {
        match self {
            HelmColumn::Name => row.name.clone(),
            HelmColumn::Namespace => row.namespace.clone(),
            HelmColumn::Revision => row.revision.to_string(),
            HelmColumn::Status => row.status.clone(),
            HelmColumn::Chart => row.chart_text(),
            HelmColumn::AppVersion => row.app_version.clone(),
            HelmColumn::Updated => row
                .updated
                .map(|time| format_age(now.duration_since(time).as_secs().max(0)))
                .unwrap_or_default(),
        }
    }

    /// How `a` and `b` compare here, ascending. Updated ascends by age -
    /// newest first - with an undated row last.
    pub(crate) fn compare(self, a: &HelmRow, b: &HelmRow) -> Ordering {
        match self {
            HelmColumn::Name => (&a.name, &a.namespace).cmp(&(&b.name, &b.namespace)),
            HelmColumn::Namespace => (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)),
            HelmColumn::Revision => a.revision.cmp(&b.revision),
            HelmColumn::Status => a.status.cmp(&b.status),
            HelmColumn::Chart => a.chart_text().cmp(&b.chart_text()),
            HelmColumn::AppVersion => a.app_version.cmp(&b.app_version),
            HelmColumn::Updated => match (a.updated, b.updated) {
                (Some(a), Some(b)) => b.cmp(&a),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
        }
    }
}

/// How a release status reads (helm-releases D3): its tone, and whether it
/// also carries an icon, so it is told apart without color.
pub(crate) fn status_tone(status: &str) -> Tone {
    match status {
        "deployed" => Tone::Good,
        "failed" => Tone::Bad,
        "uninstalling" => Tone::Warning,
        status if status.starts_with("pending-") => Tone::Warning,
        _ => Tone::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::status_tone;
    use crate::ui::style::Tone;

    #[test]
    fn statuses_take_their_tones() {
        assert_eq!(status_tone("deployed"), Tone::Good);
        assert_eq!(status_tone("failed"), Tone::Bad);
        assert_eq!(status_tone("pending-upgrade"), Tone::Warning);
        assert_eq!(status_tone("pending-install"), Tone::Warning);
        assert_eq!(status_tone("uninstalling"), Tone::Warning);
        assert_eq!(status_tone("superseded"), Tone::Neutral);
        assert_eq!(status_tone("uninstalled"), Tone::Neutral);
    }
}
