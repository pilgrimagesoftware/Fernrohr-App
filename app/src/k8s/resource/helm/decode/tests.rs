//! Decoding release payloads: Helm's own encoding, the uncompressed form it
//! also reads, the size cap, malformed input, and one payload Helm itself
//! produced (`helm install --dry-run=client -o json` with Helm 4, from the chart
//! `helm create` scaffolds, so it holds no credentials), encoded as Helm
//! stores it.

use super::{DecodeError, decode_release};
use crate::consts::HELM_RELEASE_MAX_DECODED_BYTES;
use base64::Engine as _;
use serde_json::{Value, json};
use std::io::Write as _;

/// `release` encoded as Helm stores it: JSON, gzipped, base64.
fn stored(release: &Value) -> Vec<u8> {
    let json = serde_json::to_vec(release).expect("serializes");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&json).expect("compresses");
    let compressed = gz.finish().expect("compresses");
    base64::engine::general_purpose::STANDARD
        .encode(compressed)
        .into_bytes()
}

fn release() -> Value {
    json!({
        "name": "web",
        "namespace": "shop",
        "version": 3,
        "info": {
            "status": "deployed",
            "first_deployed": "2026-11-01T10:00:00Z",
            "last_deployed": "2026-11-12T09:30:00Z",
            "description": "Upgrade complete",
            "notes": "Visit https://web.example.com"
        },
        "chart": {
            "metadata": { "name": "web", "version": "1.4.2", "appVersion": "2.0.1" },
            "templates": [{ "name": "templates/deployment.yaml", "data": "a2luZDogRGVwbG95bWVudA==" }],
            "values": { "replicaCount": 1 }
        },
        "config": { "replicaCount": 2, "image": { "tag": "2.0.1" } },
        "manifest": "---\nkind: Deployment\nmetadata:\n  name: web\n"
    })
}

#[test]
fn a_stored_release_decodes() {
    let decoded = decode_release(&stored(&release())).expect("decodes");
    assert_eq!(
        (
            decoded.name.as_str(),
            decoded.namespace.as_str(),
            decoded.version
        ),
        ("web", "shop", 3)
    );
    assert_eq!(decoded.info.status, "deployed");
    assert_eq!(
        decoded.info.description.as_deref(),
        Some("Upgrade complete")
    );
    assert_eq!(decoded.chart.name, "web");
    assert_eq!(decoded.chart.version, "1.4.2");
    assert_eq!(decoded.chart.app_version.as_deref(), Some("2.0.1"));
    let values = decoded.values.expose().expect("text");
    assert!(values.contains("replicaCount: 2"), "{values}");
    assert!(
        !values.contains("replicaCount: 1"),
        "the user's values, not the chart's defaults"
    );
    assert!(
        decoded
            .manifest
            .expose()
            .expect("text")
            .contains("kind: Deployment")
    );
}

#[test]
fn uncompressed_json_decodes_too() {
    let json = serde_json::to_vec(&release()).expect("serializes");
    let payload = base64::engine::general_purpose::STANDARD.encode(json);
    assert_eq!(
        decode_release(payload.as_bytes()).expect("decodes").version,
        3
    );
}

#[test]
fn values_and_manifest_never_print() {
    let decoded = decode_release(&stored(&release())).expect("decodes");
    let printed = format!("{decoded:?}");
    assert!(!printed.contains("replicaCount"), "{printed}");
    assert!(!printed.contains("kind: Deployment"), "{printed}");
}

#[test]
fn no_values_is_empty_not_braces() {
    let mut without = release();
    without["config"] = Value::Null;
    assert!(
        decode_release(&stored(&without))
            .expect("decodes")
            .values
            .is_empty()
    );
    without["config"] = json!({});
    assert!(
        decode_release(&stored(&without))
            .expect("decodes")
            .values
            .is_empty()
    );
}

#[test]
fn malformed_payloads_are_errors() {
    assert_eq!(
        decode_release(b"not base64!").unwrap_err(),
        DecodeError::Base64
    );
    let corrupt_gzip =
        base64::engine::general_purpose::STANDARD.encode([0x1f, 0x8b, 0x08, 0, 1, 2]);
    assert_eq!(
        decode_release(corrupt_gzip.as_bytes()).unwrap_err(),
        DecodeError::Gzip
    );
    let not_a_release = stored(&json!({ "kind": "ConfigMap" }));
    assert!(matches!(
        decode_release(&not_a_release).unwrap_err(),
        DecodeError::Json(_)
    ));
}

/// A payload that decompresses past the cap is refused, without holding more
/// than the cap.
#[test]
fn a_payload_past_the_cap_is_refused() {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![b' '; 1024 * 1024];
    for _ in 0..=(HELM_RELEASE_MAX_DECODED_BYTES / chunk.len() as u64) {
        gz.write_all(&chunk).expect("compresses");
    }
    let payload =
        base64::engine::general_purpose::STANDARD.encode(gz.finish().expect("compresses"));
    assert_eq!(
        decode_release(payload.as_bytes()).unwrap_err(),
        DecodeError::TooLarge
    );
}

#[test]
fn an_error_never_quotes_the_payload() {
    let not_a_release = stored(&json!({ "name": "web", "password": "hunter2" }));
    let message = decode_release(&not_a_release).unwrap_err().to_string();
    assert!(!message.contains("hunter2"), "{message}");
}

/// What Helm itself wrote: a Helm 4 client dry run of the scaffolded chart,
/// installed as `web` in `shop` with `replicaCount=2` and `image.tag=1.27.0`.
#[test]
fn a_payload_helm_produced_decodes() {
    let payload = include_bytes!("../fixtures/helm4-dry-run.b64");
    let decoded = decode_release(payload).expect("decodes");
    assert_eq!(
        (
            decoded.name.as_str(),
            decoded.namespace.as_str(),
            decoded.version
        ),
        ("web", "shop", 1)
    );
    assert_eq!(decoded.info.status, "pending-install");
    assert_eq!(decoded.chart.name, "demo");
    assert_eq!(decoded.chart.version, "0.1.0");
    assert_eq!(decoded.chart.app_version.as_deref(), Some("1.16.0"));
    let values = decoded.values.expose().expect("text");
    assert!(values.contains("replicaCount: 2"), "{values}");
    assert!(values.contains("tag: 1.27.0"), "{values}");
    assert!(
        decoded
            .info
            .notes
            .as_deref()
            .unwrap_or_default()
            .contains("application URL")
    );
    assert!(
        decoded
            .manifest
            .expose()
            .expect("text")
            .contains("kind: Deployment")
    );
}
