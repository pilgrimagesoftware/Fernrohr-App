use super::*;
use kube::core::response::Status;
use serde_json::json;

const SECRET: &str = "s3cr3t-t0k3n";

fn api_error(code: u16, reason: &str, message: &str) -> kube::Error {
    kube::Error::Api(Box::new(Status {
        code,
        reason: reason.to_string(),
        message: message.to_string(),
        ..Default::default()
    }))
}

fn assert_secret_free(error: &ToolError) {
    let shown = [error.to_string(), error.to_json().to_string()];
    for text in shown {
        assert!(!text.contains(SECRET), "{text} leaks the secret");
    }
}

#[test]
fn errors_serialize_with_a_stable_code_tag_and_round_trip() {
    let error = ToolError::Unavailable {
        reason: Unavailability::NotRunning,
    };
    let value = serde_json::to_value(&error).unwrap();
    assert_eq!(
        value,
        json!({"code": "unavailable", "reason": "not_running"})
    );
    assert_eq!(serde_json::from_value::<ToolError>(value).unwrap(), error);

    let error = ToolError::Kubernetes {
        status: 403,
        reason: "Forbidden".into(),
        message: "no".into(),
    };
    let value = serde_json::to_value(&error).unwrap();
    assert_eq!(value["code"], error.code());
    assert_eq!(serde_json::from_value::<ToolError>(value).unwrap(), error);
}

#[test]
fn every_code_matches_its_serialized_tag() {
    let all = [
        ToolError::Unavailable {
            reason: Unavailability::Incompatible,
        },
        ToolError::UnknownTool { name: "x".into() },
        ToolError::InvalidArguments {
            message: "x".into(),
        },
        ToolError::UnknownContext {
            context: "x".into(),
        },
        ToolError::Disconnected {
            context: "x".into(),
        },
        ToolError::UnsupportedKind { kind: "x".into() },
        ToolError::AmbiguousKind {
            kind: "x".into(),
            groups: vec![String::new()],
        },
        ToolError::UnsupportedOperation {
            kind: "x".into(),
            operation: "list".into(),
        },
        ToolError::Kubernetes {
            status: 500,
            reason: "x".into(),
            message: String::new(),
        },
        ToolError::ConnectionFailed {
            context: "x".into(),
        },
        ToolError::Denied,
        ToolError::ApprovalTimedOut,
        ToolError::Precondition {
            message: "x".into(),
        },
        ToolError::ResultTooLarge { limit: 1 },
        ToolError::UiUnavailable,
        ToolError::Internal,
    ];
    for error in all {
        assert_eq!(serde_json::to_value(&error).unwrap()["code"], error.code());
    }
}

#[test]
fn a_client_sees_the_code_fields_and_a_message() {
    let json = ToolError::UnknownContext {
        context: "dev".into(),
    }
    .to_json();
    assert_eq!(json["code"], "unknown_context");
    assert_eq!(json["context"], "dev");
    assert_eq!(json["message"], r#"no context named "dev""#);
}

#[test]
fn an_api_error_keeps_its_status_and_a_redacted_message() {
    let error = ToolError::from_kube(
        "dev",
        &api_error(
            401,
            "Unauthorized",
            &format!("token rejected: Authorization: Bearer {SECRET}"),
        ),
    );
    assert_eq!(
        error,
        ToolError::Kubernetes {
            status: 401,
            reason: "Unauthorized".into(),
            message: "token rejected: Authorization: Bearer [redacted]".into(),
        }
    );
    assert_secret_free(&error);
}

#[test]
fn an_unparsed_error_body_is_dropped_entirely() {
    let error = ToolError::from_kube(
        "dev",
        &api_error(
            502,
            UNPARSED_ERROR_REASON,
            &format!("<html>proxy auth {SECRET}</html>"),
        ),
    );
    assert_eq!(
        error,
        ToolError::Kubernetes {
            status: 502,
            reason: "Unknown".into(),
            message: String::new(),
        }
    );
    assert_secret_free(&error);
}

#[test]
fn a_client_side_failure_names_only_its_context() {
    let upstream = kube::Error::Service(Box::new(std::io::Error::other(format!(
        "exec plugin failed: --token={SECRET} in /home/me/.kube/config"
    ))));
    let error = ToolError::from_kube("dev", &upstream);
    assert_eq!(
        error,
        ToolError::ConnectionFailed {
            context: "dev".into()
        }
    );
    assert_secret_free(&error);
    assert!(!error.to_string().contains(".kube/config"));
}

#[test]
fn bad_arguments_are_reported_without_secrets() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Input {
        replicas: u32,
    }
    let parse_error =
        serde_json::from_value::<Input>(json!({"replicas": format!("token={SECRET}")}))
            .unwrap_err();
    let error = ToolError::invalid_arguments(&parse_error);
    assert_eq!(error.code(), "invalid_arguments");
    assert_secret_free(&error);
}
