// Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
// attribute macro would shadow `core::prelude::v1::test` for the plain
// synchronous tests below.
use super::streaming_title;

#[test]
fn streaming_title_names_the_pod_and_container() {
    let current = (
        "default".to_string(),
        "web-1".to_string(),
        "app".to_string(),
    );
    assert_eq!(
        streaming_title(Some(&current), || "unreachable".to_string()),
        "Logs: web-1 · app"
    );
}

#[test]
fn streaming_title_falls_back_before_a_pod_is_selected() {
    assert_eq!(streaming_title(None, || "Logs".to_string()), "Logs");
}
