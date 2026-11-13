//! A Helm release revision's stored payload, decoded (helm-releases D2).
//!
//! Helm keeps each revision in a Secret's `release` key as base64 text of
//! gzip-compressed JSON (inside Kubernetes' own base64, which the client has
//! already undone). Decoding mirrors Helm's own: base64, then gunzip when the
//! bytes start with gzip's magic number, else read them as JSON. Helm 3 and
//! Helm 4 write the same shape; Helm 4 adds fields this ignores.
//!
//! The result is trimmed: the chart's templates, files and default values are
//! skipped, never held. The two parts that can carry credentials - the user's
//! values and the rendered manifest - are kept only as [`SecretValue`], whose
//! `Debug` prints a size.

use crate::consts::HELM_RELEASE_MAX_DECODED_BYTES;
use crate::k8s::resource::secret_value::SecretValue;
use base64::Engine as _;
use serde::Deserialize;
use std::fmt;
use std::io::Read as _;

/// gzip's magic number, which Helm checks for before gunzipping.
const GZIP_MAGIC: [u8; 3] = [0x1f, 0x8b, 0x08];

/// One decoded release revision.
// UNWIRED(#370): read by the release watch (helm-releases section 2).
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct HelmRelease {
    pub(crate) name: String,
    pub(crate) namespace: String,
    /// The revision number.
    pub(crate) version: u32,
    pub(crate) info: ReleaseInfo,
    pub(crate) chart: ChartMeta,
    /// The user-supplied values (`helm get values`), as YAML. Empty when the
    /// release was installed with none.
    pub(crate) values: SecretValue,
    /// The rendered manifest (`helm get manifest`), as Helm stored it.
    pub(crate) manifest: SecretValue,
}

/// A revision's state, as `helm status` reports it.
// UNWIRED(#370): read by the release watch (helm-releases section 2).
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct ReleaseInfo {
    /// `deployed`, `superseded`, `failed`, `uninstalling`, `pending-*`, ...
    pub(crate) status: String,
    pub(crate) first_deployed: Option<String>,
    pub(crate) last_deployed: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) notes: Option<String>,
}

/// The chart a revision was made from.
// UNWIRED(#370): read by the release watch (helm-releases section 2).
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct ChartMeta {
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(rename = "appVersion")]
    pub(crate) app_version: Option<String>,
}

/// Why a payload couldn't be read. Its messages never quote the payload.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    Base64,
    Gzip,
    /// It decompresses past [`HELM_RELEASE_MAX_DECODED_BYTES`].
    TooLarge,
    /// Not a release's JSON; the parser's message, which names a position,
    /// not content.
    Json(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Base64 => f.write_str("the release isn't valid base64"),
            DecodeError::Gzip => f.write_str("the release isn't valid gzip"),
            DecodeError::TooLarge => write!(
                f,
                "the release is larger than {} MiB decompressed",
                HELM_RELEASE_MAX_DECODED_BYTES / (1024 * 1024)
            ),
            DecodeError::Json(message) => write!(f, "the release isn't a Helm release: {message}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// The parts of Helm's release JSON this reads; everything else, including
/// the chart's templates and files, is skipped by the parser.
#[derive(Deserialize)]
struct Stored {
    name: String,
    namespace: String,
    version: u32,
    #[serde(default)]
    info: ReleaseInfo,
    #[serde(default)]
    chart: StoredChart,
    #[serde(default)]
    config: Option<serde_json::Value>,
    #[serde(default)]
    manifest: String,
}

#[derive(Default, Deserialize)]
struct StoredChart {
    #[serde(default)]
    metadata: ChartMeta,
}

/// Decodes `payload`, a revision Secret's `release` value.
// UNWIRED(#370): called by the release watch (helm-releases section 2).
#[allow(dead_code)]
pub(crate) fn decode_release(payload: &[u8]) -> Result<HelmRelease, DecodeError> {
    let text: Vec<u8> = payload
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| DecodeError::Base64)?;
    let json = if bytes.starts_with(&GZIP_MAGIC) {
        gunzip(&bytes)?
    } else {
        bytes
    };
    let stored: Stored =
        serde_json::from_slice(&json).map_err(|error| DecodeError::Json(error.to_string()))?;
    drop(json);
    Ok(trimmed(stored))
}

/// `bytes` gunzipped, refused once past the size cap.
fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(HELM_RELEASE_MAX_DECODED_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|_| DecodeError::Gzip)?;
    if out.len() as u64 > HELM_RELEASE_MAX_DECODED_BYTES {
        return Err(DecodeError::TooLarge);
    }
    Ok(out)
}

fn trimmed(stored: Stored) -> HelmRelease {
    let values = match stored.config {
        Some(config) if !is_empty(&config) => serde_yaml_ng::to_string(&config).unwrap_or_default(),
        _ => String::new(),
    };
    HelmRelease {
        name: stored.name,
        namespace: stored.namespace,
        version: stored.version,
        info: stored.info,
        chart: stored.chart.metadata,
        values: SecretValue::new(values.into_bytes()),
        manifest: SecretValue::new(stored.manifest.into_bytes()),
    }
}

/// Whether `config` holds no values: Helm stores none as `null` or `{}`.
fn is_empty(config: &serde_json::Value) -> bool {
    match config {
        serde_json::Value::Null => true,
        serde_json::Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
