//! A Helm revision's status, parsed once from the label Helm writes, and which revision
//! of a release is its current one. The label itself stays on [`Revision`] for display
//! and sorting; decisions read this instead of comparing strings.

use super::table::Revision;
use crate::ui::style::Tone;

/// A revision's status, as Helm labels it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HelmStatus {
    Deployed,
    Superseded,
    Failed,
    Uninstalling,
    Uninstalled,
    /// `pending-install`, `pending-upgrade` or `pending-rollback`.
    Pending,
    /// Anything else Helm may write, kept as written.
    Other(String),
}

impl HelmStatus {
    pub(crate) fn parse(label: &str) -> Self {
        match label {
            "deployed" => Self::Deployed,
            "superseded" => Self::Superseded,
            "failed" => Self::Failed,
            "uninstalling" => Self::Uninstalling,
            "uninstalled" => Self::Uninstalled,
            pending if pending.starts_with("pending-") => Self::Pending,
            other => Self::Other(other.to_string()),
        }
    }

    /// How the status reads (helm-releases D3).
    pub(crate) fn tone(&self) -> Tone {
        match self {
            Self::Deployed => Tone::Good,
            Self::Failed => Tone::Bad,
            Self::Uninstalling | Self::Pending => Tone::Warning,
            Self::Superseded | Self::Uninstalled | Self::Other(_) => Tone::Neutral,
        }
    }
}

impl Revision {
    /// This revision's status, parsed.
    pub(crate) fn helm_status(&self) -> HelmStatus {
        HelmStatus::parse(&self.status)
    }
}

/// A release's current revision in `history` (newest first): the deployed one, else the
/// newest - what Helm itself acts on for uninstall, resources and the default selection.
pub(crate) fn current_revision(history: &[Revision]) -> Option<&Revision> {
    history
        .iter()
        .find(|revision| revision.helm_status() == HelmStatus::Deployed)
        .or(history.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(version: u32, status: &str) -> Revision {
        Revision {
            version,
            status: status.to_string(),
            secret: format!("sh.helm.release.v1.web.v{version}"),
            resource_version: "1".to_string(),
        }
    }

    #[test]
    fn statuses_parse_with_the_rest_kept() {
        assert_eq!(HelmStatus::parse("deployed"), HelmStatus::Deployed);
        assert_eq!(HelmStatus::parse("pending-upgrade"), HelmStatus::Pending);
        assert_eq!(
            HelmStatus::parse("unknown"),
            HelmStatus::Other("unknown".to_string())
        );
    }

    #[test]
    fn the_current_revision_is_the_deployed_one_else_the_newest() {
        let history = [
            revision(3, "failed"),
            revision(2, "deployed"),
            revision(1, "superseded"),
        ];
        assert_eq!(current_revision(&history).map(|r| r.version), Some(2));
        let none_deployed = [revision(2, "failed"), revision(1, "superseded")];
        assert_eq!(current_revision(&none_deployed).map(|r| r.version), Some(2));
        assert!(current_revision(&[]).is_none());
    }
}
