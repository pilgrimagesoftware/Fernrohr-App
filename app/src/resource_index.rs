use std::collections::HashMap;

/// A live-updated index for one `(cluster, kind)` watch: items in insertion
/// order (`Vec`) plus a `Uid -> index` map for O(1) lookup. Mirrors
/// `kube_runtime::watcher::Event::Apply`/`Delete` deltas.
pub struct ResourceIndex<T> {
    items: Vec<T>,
    /// Parallel to `items`: the uid at each position, so a `swap_remove` can
    /// fix up the displaced item's position without scanning `positions`.
    uids: Vec<String>,
    positions: HashMap<String, usize>,
}

impl<T> Default for ResourceIndex<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            uids: Vec::new(),
            positions: HashMap::new(),
        }
    }
}

impl<T> ResourceIndex<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Inserts a new item, or replaces the existing one with the same uid.
    pub fn apply_applied(&mut self, uid: String, item: T) {
        match self.positions.get(&uid) {
            Some(&index) => self.items[index] = item,
            None => {
                self.positions.insert(uid.clone(), self.items.len());
                self.items.push(item);
                self.uids.push(uid);
            }
        }
    }

    /// Removes the item with the given uid, if present. A delete for a uid
    /// not in the index (e.g. a duplicate or out-of-order delta) is a no-op.
    pub fn apply_deleted(&mut self, uid: &str) {
        let Some(index) = self.positions.remove(uid) else {
            return;
        };
        self.items.swap_remove(index);
        self.uids.swap_remove(index);
        if let Some(moved_uid) = self.uids.get(index) {
            self.positions.insert(moved_uid.clone(), index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_adds_a_new_item() {
        let mut index = ResourceIndex::new();
        index.apply_applied("a".into(), "pod-a");
        assert_eq!(index.items(), &["pod-a"]);
    }

    #[test]
    fn apply_on_existing_uid_updates_in_place() {
        let mut index = ResourceIndex::new();
        index.apply_applied("a".into(), "pod-a-v1");
        index.apply_applied("a".into(), "pod-a-v2");
        assert_eq!(index.items(), &["pod-a-v2"]);
    }

    #[test]
    fn delete_removes_the_item() {
        let mut index = ResourceIndex::new();
        index.apply_applied("a".into(), "pod-a");
        index.apply_applied("b".into(), "pod-b");
        index.apply_deleted("a");
        assert_eq!(index.items(), &["pod-b"]);
    }

    #[test]
    fn delete_of_missing_uid_is_a_no_op() {
        let mut index: ResourceIndex<&str> = ResourceIndex::new();
        index.apply_applied("a".into(), "pod-a");
        index.apply_deleted("does-not-exist");
        assert_eq!(index.items(), &["pod-a"]);
    }

    #[test]
    fn delete_then_reapply_and_delete_keeps_positions_consistent() {
        let mut index = ResourceIndex::new();
        index.apply_applied("a".into(), "pod-a");
        index.apply_applied("b".into(), "pod-b");
        index.apply_applied("c".into(), "pod-c");

        // swap_remove("a") moves "pod-c" (the last item) into "a"'s slot.
        index.apply_deleted("a");
        assert_eq!(index.items(), &["pod-c", "pod-b"]);

        // Deleting by uid still resolves correctly after the swap.
        index.apply_deleted("c");
        assert_eq!(index.items(), &["pod-b"]);

        index.apply_deleted("b");
        assert_eq!(index.items(), &[] as &[&str]);
    }
}
