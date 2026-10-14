//! The prompts' input rules.

use super::{parse_metadata, parse_replicas};

#[test]
fn scale_takes_a_whole_number_from_zero_up() {
    assert_eq!(parse_replicas("3"), Ok(3));
    assert_eq!(parse_replicas("0"), Ok(0));
    for bad in ["-1", "three", "2.5", ""] {
        assert!(parse_replicas(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_key_is_required_and_an_empty_value_removes_it() {
    assert_eq!(
        parse_metadata("team", "platform"),
        Ok(("team".into(), Some("platform".into())))
    );
    assert_eq!(parse_metadata("team", ""), Ok(("team".into(), None)));
    assert!(parse_metadata("", "platform").is_err());
    assert!(parse_metadata("a b", "x").is_err());
    assert!(parse_metadata("team=platform", "").is_err());
}
