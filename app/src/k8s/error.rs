//! A human-readable rendering of a [`kube::Error`], shared by every panel that
//! surfaces a cluster API failure to the user - the log stream
//! (`util::logs::stream_container_logs`) and the pod-detail fetch
//! (`k8s::resource::pod_detail::fetch::fetch_pod`) today.
//!
//! `kube::Error::Api`'s own `Display` is `"ApiError: <message> (<Debug>)"`:
//! it tells a developer everything, and a user nothing readable - the shape
//! behind `1-window-context-bar` bug 2 (`ApiError: pods "clamav-plc9g" not
//! found ... code: 404` dumped straight into a panel). [`describe`] picks the
//! API's own reason and message out of that instead, and falls back to
//! `Display` for every other [`kube::Error`] variant.
//!
//! A 404 is deliberately not special-cased here: every caller already tells a
//! "this pod is gone" 404 apart from any other failure before it ever reaches
//! [`describe`], because "not found" is its own state (see
//! `pod_detail::fetch::PodDetailState::NotFound`), naming what's missing, not a
//! message this module could word better.

/// The message half of a failure report: what a panel shows by default.
pub(crate) fn describe(error: &kube::Error) -> String {
    match error {
        kube::Error::Api(status) => format!("{}: {}", status.reason, status.message),
        other => other.to_string(),
    }
}

/// The technical half: every field `Debug` can show, kept alongside the
/// readable message rather than discarded - a panel renders both, the second
/// as selectable/copyable detail for a report that needs more than the
/// summary.
pub(crate) fn detail(error: &kube::Error) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use super::{describe, detail};
    use kube::core::response::Status;

    fn api_error(reason: &str, message: &str, code: u16) -> kube::Error {
        kube::Error::Api(Box::new(Status {
            reason: reason.to_string(),
            message: message.to_string(),
            code,
            ..Default::default()
        }))
    }

    #[test]
    fn describe_reads_an_api_errors_reason_and_message() {
        let error = api_error("Forbidden", "pods is forbidden", 403);
        assert_eq!(describe(&error), "Forbidden: pods is forbidden");
    }

    #[test]
    fn describe_falls_back_to_display_for_non_api_errors() {
        let error = kube::Error::LinesCodecMaxLineLengthExceeded;
        assert_eq!(describe(&error), error.to_string());
    }

    #[test]
    fn detail_keeps_the_full_debug_rendering() {
        let error = api_error("NotFound", "pods \"gone\" not found", 404);
        assert_eq!(detail(&error), format!("{error:?}"));
    }
}
