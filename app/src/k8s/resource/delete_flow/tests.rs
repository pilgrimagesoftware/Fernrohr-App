use super::DeleteTarget;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use kube::api::GroupVersionKind;

fn target(kind: DiscoveredKind, namespace: Option<&str>, name: &str) -> DeleteTarget {
    DeleteTarget {
        context_name: "demo".into(),
        kind,
        namespace: namespace.map(str::to_string),
        name: name.into(),
    }
}

fn core(kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: KindVerbs::default(),
    }
}

/// The question names the kind, and quotes the object and its namespace as
/// names of their own.
#[test]
fn a_namespaced_objects_question_names_it_and_its_namespace() {
    let question = target(
        core("Secret", "secrets", true),
        Some("payments"),
        "db-password",
    )
    .question();
    assert_eq!(
        question.plain(),
        "Delete Secret \u{201c}db-password\u{201d} in \u{201c}payments\u{201d}?"
    );
    assert_eq!(question.name_list(), ["db-password", "payments"]);
}

#[test]
fn a_cluster_scoped_objects_question_has_no_namespace() {
    let question = target(core("Namespace", "namespaces", false), None, "scratch").question();
    assert_eq!(
        question.plain(),
        "Delete Namespace \u{201c}scratch\u{201d}?"
    );
}

/// A pod's question warns that its controller may replace it.
#[test]
fn a_pods_question_warns_of_a_replacement() {
    let question = target(DiscoveredKind::pods(), Some("default"), "web-1").question();
    assert!(
        question
            .plain()
            .ends_with("? A controller that owns it may start a replacement."),
        "{}",
        question.plain()
    );
}

#[test]
fn the_action_says_delete_or_kill_and_names_the_object() {
    let pod = target(DiscoveredKind::pods(), Some("default"), "web-1");
    assert_eq!(pod.action(false), "Delete Pod web-1");
    assert_eq!(pod.action(true), "Kill Pod web-1");
}

/// A context that isn't connected refuses - after the call returns, so a
/// panel can send from its own action handler and hear back in an update.
#[gpui_kit::test]
async fn a_disconnected_context_refuses_after_the_call(cx: &mut gpui_kit::TestAppContext) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use gpui_kit::AppContext as _;
    use std::cell::RefCell;
    use std::rc::Rc;

    let heard = Rc::new(RefCell::new(Vec::new()));
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let during = cx.update(|cx| {
        let record = heard.clone();
        super::send_delete(
            target(DiscoveredKind::pods(), Some("default"), "web-1"),
            &connection,
            true,
            Rc::new(move |result, _| record.borrow_mut().push(result)),
            cx,
        );
        heard.borrow().len()
    });
    assert_eq!(during, 0, "nothing heard during the call");
    cx.run_until_parked();
    let heard = heard.borrow();
    assert_eq!(heard.len(), 1, "one answer after it");
    let failure = heard[0].clone().expect_err("refused");
    assert_eq!(failure.message, "demo is not connected.");
}
