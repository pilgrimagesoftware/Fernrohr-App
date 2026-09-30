//! A Secret's values never reach the panel (`resource-links` 6.2,
//! `object-detail`'s "Secret values are never shown").

use super::fixtures::{kind, object, stub_panel, target};
use super::sections::field;
use crate::k8s::resource::object_detail::redact::LAST_APPLIED_ANNOTATION;
use gpui_kit::TestAppContext;
use jiff::Timestamp;
use serde_json::json;

const PASSWORD: &str = "hunter2-hunter2";
const PASSWORD_B64: &str = "aHVudGVyMi1odW50ZXIy";
const TOKEN: &str = "plain-token-value";

fn secret() -> kube::api::DynamicObject {
    object(json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "type": "Opaque",
        "metadata": {
            "name": "app-secrets",
            "namespace": "staging",
            "annotations": {
                LAST_APPLIED_ANNOTATION: format!(
                    "{{\"stringData\":{{\"token\":\"{TOKEN}\"}},\"data\":{{\"password\":\"{PASSWORD_B64}\"}}}}"
                ),
                "team": "platform",
            },
        },
        "data": { "password": PASSWORD_B64 },
        "stringData": { "token": TOKEN },
    }))
}

/// Neither the structured view's rows nor the YAML contain any value - not
/// the encoded one, not the decoded one, not the one in the last-applied
/// annotation - and each key's size is right.
#[gpui_kit::test]
async fn no_secret_value_reaches_either_view(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let secrets = kind("", "v1", "Secret", true);
    let (window, panel) = stub_panel(
        cx,
        target(secrets.clone(), Some("staging"), "app-secrets"),
        vec![secrets],
    );
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| panel.test_set_loaded(secret(), cx))
        })
        .unwrap();

    let (sections, yaml) = window
        .update(cx, |_, _, cx| {
            let panel = panel.read(cx);
            (
                panel.sections(Timestamp::from_second(0).unwrap()),
                panel.yaml(),
            )
        })
        .unwrap();
    let yaml = yaml.expect("the secret loaded");
    let rendered_rows = format!("{sections:?}");

    for value in [PASSWORD, PASSWORD_B64, TOKEN] {
        assert!(!yaml.contains(value), "YAML leaks {value}:\n{yaml}");
        assert!(!rendered_rows.contains(value), "fields leak {value}");
    }
    assert_eq!(field(&sections, "Type").value.text(), "Opaque");
    assert_eq!(
        field(&sections, "Data").value.text(),
        format!("password: <redacted: {} bytes>", PASSWORD.len())
    );
    assert_eq!(
        field(&sections, "String Data").value.text(),
        format!("token: <redacted: {} bytes>", TOKEN.len())
    );
    assert!(
        yaml.contains("team: platform"),
        "other annotations are left alone"
    );
}
