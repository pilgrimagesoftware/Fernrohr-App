//! Where a list's store is in its lists (`list-loading-indicator` D1): still
//! on its first list, loaded, or relisting after having loaded. The object
//! list, Pods and events browser stores each keep one and feed it the
//! watcher's `Init`, `InitApply` and `InitDone`, so the three panels can't
//! disagree about what "loading" means.
//!
//! A list arrives as `Init`, any number of `InitApply`, then `InitDone`. The
//! first one takes a store from [`LoadPhase::FirstLoad`] to
//! [`LoadPhase::Loaded`]; every later `Init` - a watch restarting after a
//! reconnect, or a poll - is a [`LoadPhase::Refreshing`], which keeps the rows
//! already listed on screen.

/// A store's progress through its lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadPhase {
    /// No list has completed yet; `received` rows of the current one have
    /// arrived.
    FirstLoad { received: usize },
    /// The last list completed.
    Loaded,
    /// A list completed before, and another is under way; `received` rows of
    /// it have arrived.
    Refreshing { received: usize },
}

impl Default for LoadPhase {
    fn default() -> Self {
        Self::FirstLoad { received: 0 }
    }
}

impl LoadPhase {
    /// A list starts.
    pub fn init(&mut self) {
        *self = match self {
            Self::FirstLoad { .. } => Self::FirstLoad { received: 0 },
            Self::Loaded | Self::Refreshing { .. } => Self::Refreshing { received: 0 },
        };
    }

    /// One row of the current list arrived.
    pub fn received(&mut self) {
        match self {
            Self::FirstLoad { received } | Self::Refreshing { received } => *received += 1,
            Self::Loaded => {}
        }
    }

    /// The current list completed.
    pub fn done(&mut self) {
        *self = Self::Loaded;
    }

    /// Whether a list has ever completed: an absent row then means gone, not
    /// not listed yet.
    pub fn has_loaded(self) -> bool {
        !matches!(self, Self::FirstLoad { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::LoadPhase;

    #[test]
    fn a_first_list_counts_its_rows_then_loads() {
        let mut phase = LoadPhase::default();
        assert_eq!(phase, LoadPhase::FirstLoad { received: 0 });
        phase.init();
        phase.received();
        phase.received();
        assert_eq!(phase, LoadPhase::FirstLoad { received: 2 });
        assert!(!phase.has_loaded());
        phase.done();
        assert_eq!(phase, LoadPhase::Loaded);
        assert!(phase.has_loaded());
    }

    #[test]
    fn a_relist_after_loading_is_a_refresh() {
        let mut phase = LoadPhase::Loaded;
        phase.init();
        assert_eq!(phase, LoadPhase::Refreshing { received: 0 });
        phase.received();
        assert_eq!(phase, LoadPhase::Refreshing { received: 1 });
        assert!(phase.has_loaded(), "a refresh keeps what was loaded");
        phase.done();
        assert_eq!(phase, LoadPhase::Loaded);
    }

    #[test]
    fn a_first_list_that_restarts_is_still_a_first_load() {
        let mut phase = LoadPhase::default();
        phase.init();
        phase.received();
        // The watch restarted before its first list completed.
        phase.init();
        assert_eq!(phase, LoadPhase::FirstLoad { received: 0 });
    }

    #[test]
    fn rows_outside_a_list_count_for_nothing() {
        let mut phase = LoadPhase::Loaded;
        phase.received();
        assert_eq!(phase, LoadPhase::Loaded);
    }
}
