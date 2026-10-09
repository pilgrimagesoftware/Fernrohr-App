//! `remembered-list-sort` 3.2: a remembered sort survives a relaunch, through
//! the workspace file the app saves and loads.
//!
//! Named imports, not `use super::*`: see `util/shell.rs` on the
//! macro-expansion budget.

use super::SortDefaults;
use crate::config::workspace::{SortState, WorkspaceConfig};
use crate::util::shell::test_support::temp_workspace_path;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
async fn a_remembered_sort_survives_a_relaunch(cx: &mut TestAppContext) {
    let path = temp_workspace_path();
    let services = SortState {
        column: "type".into(),
        ascending: true,
    };
    cx.update(|cx| {
        SortDefaults::record(cx, "/Service", services.clone());
        crate::util::shell::persist::save(cx, &path);
    });

    let saved: WorkspaceConfig = crate::config::load(&path);
    assert_eq!(saved.sort_defaults.get("/Service"), Some(&services));

    // A fresh launch: nothing remembered until the file is read.
    cx.update(|cx| {
        SortDefaults::load(cx, Default::default());
        assert_eq!(SortDefaults::get(cx, "/Service"), None);
        let loaded: WorkspaceConfig = crate::config::load(&path);
        SortDefaults::load(cx, loaded.sort_defaults);
        assert_eq!(SortDefaults::get(cx, "/Service"), Some(services));
    });
}
