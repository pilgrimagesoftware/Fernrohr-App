//! `mcp-connections` Security 4 and 5: session identifiers are 128-bit,
//! secret and single-use; client names are cleaned labels.

use super::{SessionId, Sessions};

/// 128 bits, hex: 32 characters, and never the same twice.
#[test]
fn identifiers_are_128_bit_and_fresh() {
    let a = SessionId::generate().unwrap();
    let b = SessionId::generate().unwrap();
    assert_eq!(a.0.len(), 32);
    assert!(a.0.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

/// `agent-mcp` "Identifier stays out of logs": `Debug` - what a log line or
/// panic would print - never shows the identifier.
#[test]
fn an_identifier_never_shows_in_debug_output() {
    let id = SessionId::generate().unwrap();
    assert!(!format!("{id:?}").contains(&id.0));
    let sessions = Sessions::default();
    let (id, caller) = sessions.open(Some("assistant".into())).unwrap();
    assert!(
        !format!("{caller:?}").contains(&id.0),
        "the caller carries no secret"
    );
}

/// A live identifier finds its caller; a forged one, an empty one and a
/// closed one find nothing.
#[test]
fn only_a_live_identifier_finds_its_caller() {
    let sessions = Sessions::default();
    let (id, caller) = sessions.open(None).unwrap();
    let (other, _) = sessions.open(None).unwrap();
    assert_eq!(sessions.caller(&id), Some(caller.clone()));
    assert!(sessions.caller(&SessionId::generate().unwrap()).is_none());
    assert!(sessions.caller(&SessionId(String::new())).is_none());

    sessions.close(caller.agent);
    assert!(
        sessions.caller(&id).is_none(),
        "a closed session's id is dead"
    );
    assert!(sessions.caller(&other).is_some(), "other sessions live on");
}

/// Two clients reporting the same name are still two sessions, two agents.
#[test]
fn the_same_name_is_two_sessions() {
    let sessions = Sessions::default();
    let (_, a) = sessions.open(Some("assistant".into())).unwrap();
    let (_, b) = sessions.open(Some("assistant".into())).unwrap();
    assert_ne!(a.agent, b.agent);
}

/// A reported name loses control characters and is cut short; an empty one is
/// no name.
#[test]
fn client_names_are_cleaned() {
    let sessions = Sessions::default();
    let (_, caller) = sessions.open(Some("as\u{1b}[31msis\ntant".into())).unwrap();
    assert_eq!(caller.client_name.as_deref(), Some("as[31msistant"));
    let (_, caller) = sessions.open(Some("x".repeat(500))).unwrap();
    assert_eq!(caller.client_name.map(|name| name.len()), Some(64));
    let (_, caller) = sessions.open(Some(" \n ".into())).unwrap();
    assert_eq!(caller.client_name, None);
}

/// Closing every session reports each agent once, and kills every id.
#[test]
fn close_all_ends_every_session() {
    let sessions = Sessions::default();
    let (id, a) = sessions.open(None).unwrap();
    let (_, b) = sessions.open(None).unwrap();
    let mut ended = sessions.close_all();
    ended.sort_by_key(|agent| format!("{agent:?}"));
    let mut expected = vec![a.agent, b.agent];
    expected.sort_by_key(|agent| format!("{agent:?}"));
    assert_eq!(ended, expected);
    assert!(sessions.caller(&id).is_none());
}
