//! [`PidSlot`]: where a forward's transport records the pid its pidfile names,
//! so code outside the transport (which runs on the runtime, under a
//! supervisor) can tell which recorded processes a live forward still uses -
//! quiet mode's leftover sweep spares exactly those (#367).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// The pid of the process a forward's live pidfile names, or none. Shared
/// between a tunnel and its transport; a [`super::PidFile`] owned by it sets
/// it when written and clears it when dropped, so it never outlives the file.
#[derive(Clone, Debug, Default)]
pub(crate) struct PidSlot(Arc<AtomicU32>);

impl PidSlot {
    /// The pid, while a pidfile owned by this slot is live.
    pub(crate) fn get(&self) -> Option<u32> {
        match self.0.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }

    pub(super) fn set(&self, pid: u32) {
        self.0.store(pid, Ordering::Release);
    }

    /// Clears the slot if it still names `pid` - a newer pidfile's pid stays.
    pub(super) fn clear(&self, pid: u32) {
        let _ = self
            .0
            .compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}
