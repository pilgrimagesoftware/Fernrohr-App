//! Named imports, not `use super::*`: a glob re-import of `gpui_kit::*` shadows
//! the built-in `#[test]` (see `util/shell.rs`).

use super::{Sort, Step, clicked, kind_key, starting, stepped};
use crate::config::workspace::SortState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use gpui_kit::SharedString;

fn sort(column: &str, descending: bool) -> Sort {
    (SharedString::from(column.to_string()), descending)
}

fn ids(columns: &[&str]) -> Vec<SharedString> {
    columns
        .iter()
        .map(|column| SharedString::from(column.to_string()))
        .collect()
}

const NAME: &str = "name";

#[test]
fn a_header_cycles_ascending_descending_then_back_to_the_default() {
    let default = sort(NAME, false);
    let age = SharedString::from("age");
    let first = clicked(Some(&default), &age, &default);
    assert_eq!(first, sort("age", false), "a new column starts ascending");
    let second = clicked(Some(&first), &age, &default);
    assert_eq!(second, sort("age", true));
    let third = clicked(Some(&second), &age, &default);
    assert_eq!(third, default, "the third click returns to Name ascending");
}

#[test]
fn the_default_column_alternates_its_direction() {
    let default = sort(NAME, false);
    let name = SharedString::from(NAME);
    let descending = clicked(Some(&default), &name, &default);
    assert_eq!(descending, sort(NAME, true));
    assert_eq!(clicked(Some(&descending), &name, &default), default);
}

#[test]
fn the_keyboard_steps_between_columns_in_their_displayed_order() {
    // Status dragged ahead of Name: the next column is the displayed one.
    let columns = ids(&["status", NAME, "age"]);
    let default = sort(NAME, false);
    let current = sort(NAME, true);
    assert_eq!(
        stepped(Some(&current), &columns, &default, Step::Next),
        sort("age", false)
    );
    assert_eq!(
        stepped(Some(&current), &columns, &default, Step::Previous),
        sort("status", false)
    );
    // Both ends wrap.
    let last = sort("age", false);
    assert_eq!(
        stepped(Some(&last), &columns, &default, Step::Next),
        sort("status", false)
    );
}

#[test]
fn cycle_sort_takes_the_sorted_column_one_click_round() {
    let columns = ids(&[NAME, "age"]);
    let default = sort(NAME, false);
    let ascending = sort("age", false);
    let descending = stepped(Some(&ascending), &columns, &default, Step::Cycle);
    assert_eq!(descending, sort("age", true));
    assert_eq!(
        stepped(Some(&descending), &columns, &default, Step::Cycle),
        default
    );
}

#[test]
fn a_table_starts_with_its_own_sort_then_the_remembered_one_then_the_default() {
    let columns = ids(&[NAME, "age", "type"]);
    let default = sort(NAME, false);
    let remembered = || {
        Some(SortState {
            column: "type".into(),
            ascending: false,
        })
    };
    assert_eq!(
        starting(
            Some(sort("age", false)),
            remembered(),
            &columns,
            default.clone()
        ),
        sort("age", false),
        "its own saved sort first"
    );
    assert_eq!(
        starting(None, remembered(), &columns, default.clone()),
        sort("type", true),
        "then the kind's remembered sort"
    );
    assert_eq!(
        starting(None, None, &columns, default.clone()),
        default,
        "then the default"
    );
}

#[test]
fn a_sort_on_a_column_the_table_lacks_is_passed_over() {
    let columns = ids(&[NAME, "age"]);
    let default = sort(NAME, false);
    let gone = Some(SortState {
        column: "replicas".into(),
        ascending: true,
    });
    assert_eq!(
        starting(None, gone.clone(), &columns, default.clone()),
        default
    );
    assert_eq!(
        starting(
            Some(sort("replicas", true)),
            gone,
            &columns,
            default.clone()
        ),
        default
    );
}

#[test]
fn kinds_are_keyed_by_group_and_kind() {
    assert_eq!(kind_key(&DiscoveredKind::pods()), "/Pod");
    let deployments = DiscoveredKind {
        gvk: kube::api::GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    assert_eq!(kind_key(&deployments), "apps/Deployment");
}
