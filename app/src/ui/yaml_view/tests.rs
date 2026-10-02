//! What folds in a manifest, and what shows once something is folded.

use super::{YamlFolds, YamlLines, visible_lines};

const MANIFEST: &str = "\
apiVersion: v1
kind: Pod
metadata:
  name: web-1
  managedFields:
  - manager: kubectl
    operation: Update
  - manager: kubelet
spec:
  containers:
  - image: nginx
    name: web
status:
  phase: Running
";

#[test]
fn keys_and_items_with_children_fold() {
    let lines = YamlLines::parse(MANIFEST);
    let ends: Vec<(usize, Option<usize>)> = lines
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line.fold_end))
        .collect();
    assert_eq!(
        ends[2],
        (2, Some(7)),
        "metadata spans to the last managed field"
    );
    assert_eq!(
        ends[4],
        (4, Some(7)),
        "managedFields takes its same-indent items"
    );
    assert_eq!(ends[5], (5, Some(6)), "an item folds its own fields");
    assert_eq!(ends[7], (7, None), "a one-line item opens nothing");
    assert_eq!(ends[0], (0, None), "a scalar key opens nothing");
}

#[test]
fn a_folded_block_shows_as_its_one_line() {
    let lines = YamlLines::parse(MANIFEST);
    let mut folds = YamlFolds::default();
    folds.sync(MANIFEST);
    folds.toggle(4);
    let shown: Vec<usize> = visible_lines(&lines, &folds)
        .into_iter()
        .map(|(index, _, _)| index)
        .collect();
    assert_eq!(shown, [0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 13]);

    folds.fold_all(&lines);
    let shown: Vec<usize> = visible_lines(&lines, &folds)
        .into_iter()
        .map(|(index, _, _)| index)
        .collect();
    assert_eq!(
        shown,
        [0, 1, 2, 8, 12],
        "Fold All leaves the top-level keys"
    );
}

#[test]
fn folds_reset_for_a_different_manifest() {
    let mut folds = YamlFolds::default();
    folds.sync(MANIFEST);
    folds.toggle(2);
    folds.sync(MANIFEST);
    assert!(folds.is_folded(2), "the same manifest keeps its folds");
    folds.sync("kind: ConfigMap\n");
    assert!(!folds.is_folded(2));
}
