//! The flash rules apart from any table: what the journal hands a table, which
//! changes flash, how long, and a deleted row's stay.

use super::{ChangeJournal, FlashRow, RowChange, RowFlashes, strength};
use crate::consts::{ROW_FLASH, ROW_FLASH_ALPHA, ROW_FLASH_HOLD};
use std::time::{Duration, Instant};

/// A row: its uid, what it shows, and an age that changes with time alone.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    uid: &'static str,
    shown: &'static str,
    age: u32,
}

impl FlashRow for Row {
    fn flash_uid(&self) -> &str {
        self.uid
    }

    fn shows_same(&self, other: &Self) -> bool {
        self.shown == other.shown
    }
}

fn row(uid: &'static str, shown: &'static str) -> Row {
    Row { uid, shown, age: 0 }
}

/// A tracker that has read `journal` once, as a table's first draw does.
fn primed(journal: &ChangeJournal, rows: &[Row], at: Instant) -> RowFlashes<Row> {
    let mut flashes = RowFlashes::default();
    flashes.update(journal.since(flashes.seen()), &[], rows.to_vec(), at);
    flashes
}

#[test]
fn a_tables_first_read_skips_what_came_before() {
    let mut journal = ChangeJournal::default();
    journal.record("a".into(), RowChange::Added);
    let (changes, _) = journal.since(None);
    assert!(changes.is_empty(), "opening a panel never flashes old news");
}

#[test]
fn a_later_read_has_only_whats_new() {
    let mut journal = ChangeJournal::default();
    journal.record("a".into(), RowChange::Added);
    let (_, seen) = journal.since(None);
    journal.record("b".into(), RowChange::Modified);
    assert_eq!(
        journal.since(Some(seen)).0,
        [("b".to_string(), RowChange::Modified)]
    );
}

#[test]
fn added_modified_and_deleted_rows_flash_their_change() {
    let at = Instant::now();
    let mut journal = ChangeJournal::default();
    let old = [row("a", "1"), row("b", "1")];
    let mut flashes = primed(&journal, &old, at);
    journal.record("c".into(), RowChange::Added);
    journal.record("a".into(), RowChange::Modified);
    journal.record("b".into(), RowChange::Deleted);
    let drawn = flashes.update(
        journal.since(flashes.seen()),
        &old,
        vec![row("a", "2"), row("c", "1")],
        at,
    );
    assert_eq!(flashes.change("c"), Some(RowChange::Added));
    assert_eq!(flashes.change("a"), Some(RowChange::Modified));
    assert_eq!(flashes.change("b"), Some(RowChange::Deleted));
    assert!(
        drawn.contains(&row("b", "1")),
        "the deleted row is still drawn"
    );
}

/// A change that leaves every drawn cell alike - an age moving on, a
/// `resourceVersion` bump - doesn't flash.
#[test]
fn a_change_that_draws_the_same_does_not_flash() {
    let at = Instant::now();
    let mut journal = ChangeJournal::default();
    let old = [row("a", "1")];
    let mut flashes = primed(&journal, &old, at);
    journal.record("a".into(), RowChange::Modified);
    let mut aged = row("a", "1");
    aged.age = 30;
    flashes.update(journal.since(flashes.seen()), &old, vec![aged], at);
    assert_eq!(flashes.change("a"), None);
}

#[test]
fn a_flash_ends_and_a_deleted_row_goes_with_it() {
    let at = Instant::now();
    let mut journal = ChangeJournal::default();
    let old = [row("a", "1")];
    let mut flashes = primed(&journal, &old, at);
    journal.record("a".into(), RowChange::Deleted);
    let drawn = flashes.update(journal.since(flashes.seen()), &old, Vec::new(), at);
    assert_eq!(drawn.len(), 1, "drawn on for its flash");

    let later = at + ROW_FLASH;
    let drawn = flashes.update(journal.since(flashes.seen()), &drawn, Vec::new(), later);
    assert!(drawn.is_empty(), "and gone once it ends");
    assert_eq!(flashes.change("a"), None);
    assert_eq!(
        flashes.next_redraw(false),
        None,
        "nothing left to redraw for"
    );
}

#[test]
fn a_flash_holds_then_fades_or_just_ends_with_reduced_motion() {
    assert_eq!(strength(Duration::ZERO, false), ROW_FLASH_ALPHA);
    assert_eq!(strength(ROW_FLASH_HOLD, false), ROW_FLASH_ALPHA);
    let midway = ROW_FLASH_HOLD + (ROW_FLASH - ROW_FLASH_HOLD) / 2;
    let fading = strength(midway, false);
    assert!(fading > 0.0 && fading < ROW_FLASH_ALPHA, "fading: {fading}");
    assert_eq!(
        strength(midway, true),
        ROW_FLASH_ALPHA,
        "reduced motion holds"
    );
    assert_eq!(strength(ROW_FLASH, false), 0.0);
    assert_eq!(strength(ROW_FLASH, true), 0.0);
}

/// A deleted row is drawn back where it was, not at the end, so an unsorted
/// list doesn't jump while it flashes.
#[test]
fn a_deleted_row_is_drawn_where_it_was() {
    let at = Instant::now();
    let mut journal = ChangeJournal::default();
    let old = [row("a", "1"), row("b", "1"), row("c", "1")];
    let mut flashes = primed(&journal, &old, at);
    journal.record("b".into(), RowChange::Deleted);
    let drawn = flashes.update(
        journal.since(flashes.seen()),
        &old,
        vec![row("a", "1"), row("c", "1")],
        at,
    );
    let uids: Vec<&str> = drawn.iter().map(|row| row.uid).collect();
    assert_eq!(uids, ["a", "b", "c"]);
    assert!(flashes.is_ghost("b") && !flashes.is_ghost("a"));
}
