// Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
// attribute macro would shadow `core::prelude::v1::test`.
use super::{LineSource, LogLine, pod_colour};
use crate::consts::LABEL_LOGS_POD_HUES;

/// `selector-logs-panel` 3.1: a tagged line keeps its tag and text apart, and
/// still reads as `tag │ text` wherever it's shown or compared as one.
#[test]
fn a_tagged_line_keeps_its_source_apart_from_its_text() {
    let line = LogLine::tagged(LineSource::new("web-1/app", "web-1"), "ready".into());
    assert_eq!(line.text(), "ready");
    let source = line.source().expect("tagged");
    assert_eq!((source.tag(), source.pod()), ("web-1/app", "web-1"));
    assert_eq!(line.to_string(), "web-1/app \u{2502} ready");
    assert_eq!(line, "web-1/app \u{2502} ready");
    assert_ne!(line, "ready", "the tag is part of what it reads as");
    assert_ne!(line, "web-1 \u{2502} ready");

    let plain = LogLine::plain("ready".into());
    assert_eq!(plain.source(), None);
    assert_eq!(plain, "ready");
    assert_eq!(plain.to_string(), "ready");
}

/// 3.2: a pod's colour is a function of its name alone - the same every time,
/// and the same for each of its containers, whose tags differ but whose pod
/// doesn't.
#[test]
fn a_pods_colour_is_stable_and_shared_by_its_containers() {
    for pod in ["web-1", "db-0", "checkout-7d9f8c-x2x9q", ""] {
        let colour = pod_colour(pod);
        assert!(colour < LABEL_LOGS_POD_HUES.len());
        assert_eq!(pod_colour(pod), colour, "{pod} keeps its colour");
    }
    let app = LineSource::new("web-1/app", "web-1");
    let sidecar = LineSource::new("web-1/sidecar", "web-1");
    assert_eq!(pod_colour(app.pod()), pod_colour(sidecar.pod()));
    let spread: std::collections::HashSet<usize> =
        (0..40).map(|n| pod_colour(&format!("web-{n}"))).collect();
    assert!(spread.len() > 4, "pods spread over the palette: {spread:?}");
}
