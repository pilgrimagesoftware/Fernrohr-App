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
    // `metadata.managedFields`, line 4, starts folded (#182).
    folds.sync(MANIFEST);
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

/// An object's manifest with `metadata.managedFields`, at `version`.
fn with_managed_fields(version: &str) -> String {
    format!(
        "apiVersion: v1\n\
         kind: ConfigMap\n\
         metadata:\n  \
           name: settings\n  \
           resourceVersion: \"{version}\"\n  \
           managedFields:\n  \
           - manager: kubectl\n    \
             operation: Apply\n  \
           - manager: controller\n    \
             operation: Update\n  \
           uid: u1\n\
         data:\n  \
           mode: fast\n"
    )
}

/// The line `managedFields:` opens on in [`with_managed_fields`].
const MANAGED_FIELDS_LINE: usize = 5;

#[test]
fn managed_fields_is_found_only_under_the_objects_own_metadata() {
    let lines = YamlLines::parse(&with_managed_fields("1"));
    assert_eq!(lines.managed_fields_line(), Some(MANAGED_FIELDS_LINE));
    let nested = "kind: Deployment\nspec:\n  template:\n    metadata:\n      managedFields:\n      - manager: x\n";
    assert_eq!(YamlLines::parse(nested).managed_fields_line(), None);
    assert_eq!(
        YamlLines::parse("kind: ConfigMap\ndata:\n  a: b\n").managed_fields_line(),
        None
    );
}

/// #182: the first time a manifest shows, `metadata.managedFields` is folded,
/// and nothing else is.
#[test]
fn managed_fields_starts_folded() {
    let mut folds = YamlFolds::default();
    folds.sync(&with_managed_fields("1"));
    assert!(folds.is_folded(MANAGED_FIELDS_LINE));
    let lines = YamlLines::parse(&with_managed_fields("1"));
    let folded: Vec<usize> = lines
        .foldable()
        .filter(|line| folds.is_folded(*line))
        .collect();
    assert_eq!(folded, [MANAGED_FIELDS_LINE], "only managedFields");
}

/// Unfolded by the user, it stays open when an update changes the manifest -
/// and left folded, it stays folded, even where the update moves its line.
#[test]
fn managed_fields_keeps_the_users_choice_across_updates() {
    let mut folds = YamlFolds::default();
    folds.sync(&with_managed_fields("1"));
    folds.toggle(MANAGED_FIELDS_LINE);
    assert!(!folds.is_folded(MANAGED_FIELDS_LINE), "unfolded");
    folds.sync(&with_managed_fields("2"));
    assert!(
        !folds.is_folded(MANAGED_FIELDS_LINE),
        "still open after the update"
    );

    let mut folds = YamlFolds::default();
    folds.sync(&with_managed_fields("1"));
    let moved =
        with_managed_fields("2").replace("metadata:\n", "metadata:\n  labels:\n    app: web\n");
    folds.sync(&moved);
    let line = YamlLines::parse(&moved)
        .managed_fields_line()
        .expect("still there");
    assert_eq!(line, MANAGED_FIELDS_LINE + 2, "the update moved it");
    assert!(folds.is_folded(line), "and it is still folded there");
}

/// Unfold All opens it for good; Fold All folds it again.
#[test]
fn unfold_all_and_fold_all_set_managed_fields_too() {
    let mut folds = YamlFolds::default();
    folds.sync(&with_managed_fields("1"));
    folds.unfold_all();
    folds.sync(&with_managed_fields("2"));
    assert!(!folds.is_folded(MANAGED_FIELDS_LINE));
    folds.fold_all(&YamlLines::parse(&with_managed_fields("2")));
    folds.sync(&with_managed_fields("3"));
    assert!(folds.is_folded(MANAGED_FIELDS_LINE));
}
