//! A pod's `status.phase`, parsed once from the string the API reports, so a decision
//! about it is a `match` the compiler checks rather than a string compare.

/// A pod's lifecycle phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PodPhase {
    Pending,
    Running,
    Succeeded,
    Failed,
    /// `Unknown`, or a value this build doesn't know, kept as reported.
    Unknown(String),
}

impl PodPhase {
    pub(crate) fn parse(phase: &str) -> Self {
        match phase {
            "Pending" => Self::Pending,
            "Running" => Self::Running,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            other => Self::Unknown(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PodPhase;

    #[test]
    fn phases_parse_with_the_rest_kept() {
        assert_eq!(PodPhase::parse("Running"), PodPhase::Running);
        assert_eq!(
            PodPhase::parse("Unknown"),
            PodPhase::Unknown("Unknown".into())
        );
        assert_eq!(PodPhase::parse(""), PodPhase::Unknown(String::new()));
    }
}
