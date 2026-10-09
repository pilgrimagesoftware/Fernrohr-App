//! `remembered-list-sort` in the events browser: it opens on the remembered
//! events sort, else newest first; its own saved sort wins and records
//! nothing; and the keyboard's sort commands are remembered under `events`.

use super::super::columns::EventColumn;
use super::*;
use crate::config::workspace::SortState;
use crate::util::shell::SortDefaults;

const KEY: &str = crate::ui::list_sort::EVENTS_KEY;

fn remember(cx: &mut TestAppContext, column: &str, ascending: bool) {
    cx.update(|cx| {
        SortDefaults::record(
            cx,
            KEY,
            SortState {
                column: column.into(),
                ascending,
            },
        )
    });
}

fn sort_of(h: &mut Harness) -> Option<(EventColumn, ColumnSort)> {
    h.vcx.update(|_, cx| h.panel.read(cx).sort(cx))
}

fn remembered(h: &mut Harness) -> Option<SortState> {
    h.vcx.update(|_, cx| SortDefaults::get(cx, KEY))
}

#[gpui_kit::test]
async fn an_events_browser_opens_with_the_remembered_sort(cx: &mut TestAppContext) {
    remember(cx, "reason", true);
    let mut h = harness(cx, fixture());
    assert_eq!(
        sort_of(&mut h),
        Some((EventColumn::Reason, ColumnSort::Ascending))
    );
}

#[gpui_kit::test]
async fn never_sorted_it_opens_newest_first_and_records_nothing(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    assert_eq!(
        sort_of(&mut h),
        Some((EventColumn::LastSeen, ColumnSort::Ascending))
    );
    assert_eq!(remembered(&mut h), None);
}

#[gpui_kit::test]
async fn a_restored_browser_keeps_its_own_sort_and_records_nothing(cx: &mut TestAppContext) {
    remember(cx, "reason", true);
    let mut h = harness_with(cx, fixture(), |panel| {
        panel.initial_sort = Some((EventColumn::Count, ColumnSort::Descending));
    });
    assert_eq!(
        sort_of(&mut h),
        Some((EventColumn::Count, ColumnSort::Descending))
    );
    assert_eq!(
        remembered(&mut h),
        Some(SortState {
            column: "reason".into(),
            ascending: true,
        })
    );
}

#[gpui_kit::test]
async fn the_keyboard_cycles_the_events_sort_and_remembers_it(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    focus_table(&mut h);

    press(&mut h.vcx, "o");
    assert_eq!(
        sort_of(&mut h),
        Some((EventColumn::LastSeen, ColumnSort::Descending)),
        "the default column alternates"
    );
    press(&mut h.vcx, "shift-.");
    assert_eq!(
        sort_of(&mut h),
        Some((EventColumn::Type, ColumnSort::Ascending))
    );
    assert_eq!(
        remembered(&mut h),
        Some(SortState {
            column: "type".into(),
            ascending: true,
        })
    );
    press(&mut h.vcx, "o o");
    assert_eq!(
        sort_of(&mut h),
        Some((EventColumn::LastSeen, ColumnSort::Ascending)),
        "returning to the default restores newest first"
    );
}
