//! Flashing the list rows a watch event changes (#275), k9s-style: a row that
//! appears flashes the good tone, one whose drawn cells change the warning tone,
//! and one that goes the bad tone - drawn on for its flash, then removed.
//!
//! Two halves, so the object list, the Pods table and the events browser share
//! one machinery:
//! - A store records its live changes in a [`ChangeJournal`]: watch events
//!   after its list has loaded, never the first list, a relist or a resync, so
//!   those never flash. Sequence-numbered rather than drained, so every panel
//!   over a store reads every change.
//! - A table's delegate keeps a [`RowFlashes`], which reads the journal when the
//!   panel hands it new rows - while the rows it last drew are still at hand -
//!   and flashes only what changed what a row shows ([`FlashRow::shows_same`]).
//!
//! A flash holds, then fades out over [`ROW_FLASH`]. The panel keeps one timer
//! while any row flashes ([`redraw_after`]), however many do. With reduced
//! motion a flash holds and then ends, without fading.

use crate::consts::{
    ROW_FLASH, ROW_FLASH_ALPHA, ROW_FLASH_HOLD, ROW_FLASH_JOURNAL, ROW_FLASH_TICK,
};
use crate::ui::style::{self, Tone};
use gpui_kit::{App, Context, Hsla, Task};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// What a watch event did to a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowChange {
    Added,
    Modified,
    Deleted,
}

impl RowChange {
    /// The tone the row flashes in.
    pub(crate) fn tone(self) -> Tone {
        match self {
            RowChange::Added => Tone::Good,
            RowChange::Modified => Tone::Warning,
            RowChange::Deleted => Tone::Bad,
        }
    }
}

/// The changes a table reads from its store since its last read, and where its
/// next read starts.
pub(crate) type Changes = (Vec<(String, RowChange)>, u64);

/// A store's live changes, by uid, each with a sequence number.
#[derive(Debug, Default)]
pub(crate) struct ChangeJournal {
    next: u64,
    entries: VecDeque<(u64, String, RowChange)>,
}

impl ChangeJournal {
    /// Records that a live watch event did `change` to `uid`'s object. Keeps
    /// the last [`ROW_FLASH_JOURNAL`] changes.
    pub(crate) fn record(&mut self, uid: String, change: RowChange) {
        self.entries.push_back((self.next, uid, change));
        self.next += 1;
        while self.entries.len() > ROW_FLASH_JOURNAL {
            self.entries.pop_front();
        }
    }

    /// The changes from `seen` on. A table's first read (`seen` `None`) starts
    /// after everything so far, so opening a panel never flashes old news.
    pub(crate) fn since(&self, seen: Option<u64>) -> Changes {
        let changes = match seen {
            None => Vec::new(),
            Some(seen) => self
                .entries
                .iter()
                .filter(|(seq, _, _)| *seq >= seen)
                .map(|(_, uid, change)| (uid.clone(), *change))
                .collect(),
        };
        (changes, self.next)
    }
}

/// A row a table can flash: its object's uid, and whether two versions of it
/// draw the same.
pub(crate) trait FlashRow: Clone {
    fn flash_uid(&self) -> &str;

    /// Whether `self` and `other` draw the same cells, text that changes with
    /// time alone (an age) aside - a change that leaves them alike, such as a
    /// `resourceVersion` bump, doesn't flash.
    fn shows_same(&self, other: &Self) -> bool;
}

#[derive(Clone, Copy, Debug)]
struct Flash {
    change: RowChange,
    started: Instant,
}

/// A table's flashing rows, and its deleted rows still drawn for their flash.
pub(crate) struct RowFlashes<R> {
    seen: Option<u64>,
    flashes: HashMap<String, Flash>,
    /// Deleted rows, kept until their flash ends.
    ghosts: Vec<R>,
    /// When the table last took rows, for each flash's strength.
    now: Option<Instant>,
    /// The panel's one timer that redraws it while rows flash.
    redraw: Option<Task<()>>,
}

impl<R> Default for RowFlashes<R> {
    fn default() -> Self {
        Self {
            seen: None,
            flashes: HashMap::new(),
            ghosts: Vec::new(),
            now: None,
            redraw: None,
        }
    }
}

impl<R: FlashRow> RowFlashes<R> {
    /// Where this table's next read of its store's journal starts.
    pub(crate) fn seen(&self) -> Option<u64> {
        self.seen
    }

    /// Flashes what `changes` did between `old` (the rows the table drew last)
    /// and `new`, at `now`, and ends the flashes that have run their course.
    /// Returns `new` with the deleted rows still flashing added back.
    pub(crate) fn update(
        &mut self,
        changes: Changes,
        old: &[R],
        mut new: Vec<R>,
        now: Instant,
    ) -> Vec<R> {
        let (changes, next) = changes;
        self.seen = Some(next);
        self.now = Some(now);
        self.flashes
            .retain(|_, flash| now.saturating_duration_since(flash.started) < ROW_FLASH);
        if !changes.is_empty() {
            let before: HashMap<&str, &R> = old.iter().map(|row| (row.flash_uid(), row)).collect();
            let after: HashMap<&str, &R> = new.iter().map(|row| (row.flash_uid(), row)).collect();
            let mut deleted = Vec::new();
            for (uid, change) in changes {
                let (was, is) = (before.get(uid.as_str()), after.get(uid.as_str()));
                let flashes = match (change, was, is) {
                    (RowChange::Added, _, Some(_)) => true,
                    (RowChange::Modified, Some(was), Some(is)) => !was.shows_same(is),
                    (RowChange::Deleted, Some(was), None) => {
                        deleted.push((*was).clone());
                        true
                    }
                    _ => false,
                };
                if flashes {
                    self.flashes.insert(
                        uid,
                        Flash {
                            change,
                            started: now,
                        },
                    );
                }
            }
            for row in deleted {
                self.ghosts
                    .retain(|ghost| ghost.flash_uid() != row.flash_uid());
                self.ghosts.push(row);
            }
        }
        // A deleted row leaves with its flash, or as soon as it is back.
        let flashes = &self.flashes;
        self.ghosts.retain(|ghost| {
            let uid = ghost.flash_uid();
            flashes
                .get(uid)
                .is_some_and(|flash| flash.change == RowChange::Deleted)
                && !new.iter().any(|row| row.flash_uid() == uid)
        });
        new.extend(self.ghosts.iter().cloned());
        new
    }

    /// The change `uid`'s row is flashing for, if it is. Test-only: the
    /// tables draw [`Self::tint`].
    #[cfg(test)]
    pub(crate) fn change(&self, uid: &str) -> Option<RowChange> {
        let now = self.now?;
        self.flashes
            .get(uid)
            .filter(|flash| now.saturating_duration_since(flash.started) < ROW_FLASH)
            .map(|flash| flash.change)
    }

    /// `uid`'s row background now: its change's tone at the flash's strength,
    /// or `None` while it isn't flashing.
    pub(crate) fn tint(&self, uid: &str, cx: &App) -> Option<Hsla> {
        let (flash, now) = (self.flashes.get(uid)?, self.now?);
        let alpha = strength(
            now.saturating_duration_since(flash.started),
            cx.reduce_motion(),
        );
        (alpha > 0.0).then(|| style::status(flash.change.tone(), cx).opacity(alpha))
    }

    /// How long until the panel should draw again for its flashes: a fade's
    /// next step, or with `reduce_motion` the earliest flash's end. `None`
    /// while nothing flashes.
    pub(crate) fn next_redraw(&self, reduce_motion: bool) -> Option<Duration> {
        let now = self.now?;
        let remaining = self
            .flashes
            .values()
            .map(|flash| ROW_FLASH.saturating_sub(now.saturating_duration_since(flash.started)))
            .min()?;
        Some(if reduce_motion {
            remaining
        } else {
            remaining.min(ROW_FLASH_TICK)
        })
    }

    /// Keeps `redraw` as the panel's one pending redraw, replacing the last.
    pub(crate) fn set_redraw(&mut self, redraw: Option<Task<()>>) {
        self.redraw = redraw;
    }
}

/// A flash's strength `elapsed` into it: full through [`ROW_FLASH_HOLD`], then
/// easing to nothing by [`ROW_FLASH`] - or, with `reduce_motion`, full until it
/// ends, without the fade.
pub(crate) fn strength(elapsed: Duration, reduce_motion: bool) -> f32 {
    if elapsed >= ROW_FLASH {
        return 0.0;
    }
    if reduce_motion || elapsed <= ROW_FLASH_HOLD {
        return ROW_FLASH_ALPHA;
    }
    let fade =
        (elapsed - ROW_FLASH_HOLD).as_secs_f32() / (ROW_FLASH - ROW_FLASH_HOLD).as_secs_f32();
    ROW_FLASH_ALPHA * (1.0 - fade)
}

/// The panel's redraw `delay` from now, as a task it keeps until it draws
/// again - `None` when nothing flashes.
pub(crate) fn redraw_after<V: 'static>(
    delay: Option<Duration>,
    cx: &mut Context<V>,
) -> Option<Task<()>> {
    let delay = delay?;
    Some(cx.spawn(async move |this, cx| {
        cx.background_executor().timer(delay).await;
        let _ = this.update(cx, |_, cx| cx.notify());
    }))
}

#[cfg(test)]
mod tests;
