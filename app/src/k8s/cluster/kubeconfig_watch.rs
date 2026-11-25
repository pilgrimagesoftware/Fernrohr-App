//! Noticing that the kubeconfig changed while the app runs (#320).
//!
//! The context list used to be read once, when a picker opened. [`start`] now polls the
//! files `Kubeconfig::read` merges (`$KUBECONFIG`'s list, else `~/.kube/config`) and bumps
//! [`KubeconfigRevision`] once an edit has settled, the same shape as `TunnelsRevision`:
//! views that cache the context list observe it and re-read. Polling a few file stats
//! needs no new dependency and behaves the same on every platform; an editor's
//! write-then-rename shows up as a changed length or modification time just as an in-place
//! write does.
//!
//! The detection ([`ChangeDetector`]) and the list merge ([`merge_contexts`]) are pure, so
//! tests cover them without a watcher, a clock or a real kubeconfig.

use gpui_kit::{App, Global};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// How often the files are checked. A change is acted on one poll after it is first seen
/// (see [`ChangeDetector`]), so a reload follows an edit by two to four seconds.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Bumped once per settled change to the kubeconfig files. The value itself means nothing;
/// observers only care that it moved.
#[derive(Default)]
pub struct KubeconfigRevision(u64);

impl Global for KubeconfigRevision {}

/// Tells every observer that the kubeconfig changed on disk.
pub fn notify_kubeconfig_changed(cx: &mut App) {
    let next = cx
        .try_global::<KubeconfigRevision>()
        .map_or(1, |revision| revision.0 + 1);
    cx.set_global(KubeconfigRevision(next));
}

/// What a kubeconfig file looked like on one poll: `None` when it is missing or unreadable.
type FileStamp = Option<(SystemTime, u64)>;

/// Every watched file with its [`FileStamp`], in a fixed order.
pub(crate) type Fingerprint = Vec<(PathBuf, FileStamp)>;

/// The stamp of each of `files`. Only metadata is read, never contents.
pub(crate) fn fingerprint(files: &[PathBuf]) -> Fingerprint {
    files
        .iter()
        .map(|file| {
            let stamp = std::fs::metadata(file)
                .ok()
                .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
            (file.clone(), stamp)
        })
        .collect()
}

/// Debounces fingerprints into reload decisions: a change is reported only once the
/// fingerprint has been the same on two polls in a row, so a save that takes several writes
/// (or `kubectl config` rewriting the file) triggers one reload after it finishes, not a
/// read of a half-written file.
#[derive(Debug)]
pub(crate) struct ChangeDetector {
    /// The fingerprint the context list was last loaded for.
    loaded: Fingerprint,
    /// A newer fingerprint seen on the previous poll, waiting to prove stable.
    pending: Option<Fingerprint>,
}

impl ChangeDetector {
    /// A detector whose context list was loaded when the files looked like `loaded`.
    pub(crate) fn new(loaded: Fingerprint) -> Self {
        Self {
            loaded,
            pending: None,
        }
    }

    /// Feeds one poll's fingerprint. `true` means the files changed since the last reload
    /// and have stopped changing: reload now.
    pub(crate) fn observe(&mut self, current: Fingerprint) -> bool {
        if current == self.loaded {
            self.pending = None;
            return false;
        }
        if self.pending.as_ref() == Some(&current) {
            self.loaded = current;
            self.pending = None;
            return true;
        }
        self.pending = Some(current);
        false
    }
}

/// The context list after a re-read.
///
/// - `fresh` is the new read. When it failed (a file caught mid-write, say) and `previous`
///   was a list, the previous list stays: a transient error must not blank the picker.
/// - A context in `previous` that `fresh` no longer has stays, after the fresh ones, while it
///   is in `open`: its session keeps working, because a connection holds its own config.
///   It drops out at the first re-read after it disconnects.
pub(crate) fn merge_contexts(
    previous: &Result<Vec<String>, String>,
    fresh: Result<Vec<String>, String>,
    open: &[String],
) -> Result<Vec<String>, String> {
    let mut merged = match (fresh, previous) {
        (Ok(fresh), _) => fresh,
        (Err(_), Ok(previous)) => return Ok(previous.clone()),
        (Err(error), Err(_)) => return Err(error),
    };
    if let Ok(previous) = previous {
        let retained: Vec<String> = previous
            .iter()
            .filter(|name| open.contains(name) && !merged.contains(name))
            .cloned()
            .collect();
        merged.extend(retained);
    }
    Ok(merged)
}

/// Starts watching the kubeconfig files for the life of the app. Call once, from the main
/// thread, after the global state exists.
pub fn start(cx: &mut App) {
    let files = super::oidc::kubeconfig_files();
    let mut detector = ChangeDetector::new(fingerprint(&files));
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let files = files.clone();
            let current = cx
                .background_executor()
                .spawn(async move { fingerprint(&files) })
                .await;
            if detector.observe(current) {
                cx.update(notify_kubeconfig_changed);
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(secs: u64, len: u64) -> FileStamp {
        Some((SystemTime::UNIX_EPOCH + Duration::from_secs(secs), len))
    }

    fn prints(stamp: FileStamp) -> Fingerprint {
        vec![(PathBuf::from("/kube/config"), stamp)]
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn an_unchanged_fingerprint_never_reloads() {
        let mut detector = ChangeDetector::new(prints(stamp(1, 10)));
        assert!(!detector.observe(prints(stamp(1, 10))));
        assert!(!detector.observe(prints(stamp(1, 10))));
    }

    #[test]
    fn a_change_reloads_once_it_has_settled() {
        let mut detector = ChangeDetector::new(prints(stamp(1, 10)));
        assert!(
            !detector.observe(prints(stamp(2, 12))),
            "first sighting waits"
        );
        assert!(
            detector.observe(prints(stamp(2, 12))),
            "stable on the next poll"
        );
        assert!(
            !detector.observe(prints(stamp(2, 12))),
            "and reloads only once"
        );
    }

    #[test]
    fn a_file_still_changing_keeps_waiting() {
        let mut detector = ChangeDetector::new(prints(stamp(1, 10)));
        assert!(!detector.observe(prints(stamp(2, 12))));
        assert!(!detector.observe(prints(stamp(3, 20))));
        assert!(detector.observe(prints(stamp(3, 20))));
    }

    #[test]
    fn a_change_that_reverts_before_settling_is_dropped() {
        let mut detector = ChangeDetector::new(prints(stamp(1, 10)));
        assert!(!detector.observe(prints(stamp(2, 12))));
        assert!(!detector.observe(prints(stamp(1, 10))));
        assert!(!detector.observe(prints(stamp(1, 10))));
    }

    #[test]
    fn a_file_appearing_or_disappearing_counts_as_a_change() {
        let mut detector = ChangeDetector::new(prints(None));
        assert!(!detector.observe(prints(stamp(1, 10))));
        assert!(detector.observe(prints(stamp(1, 10))));
        assert!(!detector.observe(prints(None)));
        assert!(detector.observe(prints(None)));
    }

    #[test]
    fn fingerprint_reflects_a_real_file_and_a_missing_one() {
        let present = crate::util::test_paths::temp_path("kubeconfig-watch");
        let missing = crate::util::test_paths::temp_path("kubeconfig-watch-missing");
        std::fs::write(&present, "a: 1\n").unwrap();

        let before = fingerprint(&[present.clone(), missing.clone()]);
        assert!(before[0].1.is_some());
        assert_eq!(before[1].1, None);

        std::fs::write(&present, "a: 1\nb: 2\n").unwrap();
        assert_ne!(before, fingerprint(&[present.clone(), missing]));
        let _ = std::fs::remove_file(&present);
    }

    #[test]
    fn merge_takes_the_fresh_list_in_its_order() {
        let merged = merge_contexts(&Ok(names(&["a", "b"])), Ok(names(&["c", "a", "d"])), &[]);
        assert_eq!(merged, Ok(names(&["c", "a", "d"])));
    }

    #[test]
    fn merge_drops_a_vanished_context_that_is_not_connected() {
        let merged = merge_contexts(&Ok(names(&["a", "b"])), Ok(names(&["a"])), &[]);
        assert_eq!(merged, Ok(names(&["a"])));
    }

    #[test]
    fn merge_keeps_a_vanished_context_that_is_connected() {
        let merged = merge_contexts(
            &Ok(names(&["a", "b"])),
            Ok(names(&["c"])),
            &names(&["b", "elsewhere"]),
        );
        assert_eq!(merged, Ok(names(&["c", "b"])));
    }

    #[test]
    fn merge_does_not_duplicate_a_connected_context_that_is_still_there() {
        let merged = merge_contexts(&Ok(names(&["a"])), Ok(names(&["a"])), &names(&["a"]));
        assert_eq!(merged, Ok(names(&["a"])));
    }

    #[test]
    fn merge_keeps_the_previous_list_when_the_read_fails() {
        let merged = merge_contexts(&Ok(names(&["a"])), Err("half written".into()), &[]);
        assert_eq!(merged, Ok(names(&["a"])));
    }

    #[test]
    fn merge_reports_the_error_when_there_was_no_list_to_keep() {
        let merged = merge_contexts(&Err("old".into()), Err("new".into()), &[]);
        assert_eq!(merged, Err("new".to_string()));
    }

    #[test]
    fn merge_recovers_from_an_earlier_error() {
        let merged = merge_contexts(&Err("old".into()), Ok(names(&["a"])), &names(&["x"]));
        assert_eq!(merged, Ok(names(&["a"])));
    }
}
