//! `namespace-sets` 3.3 and "Sets survive a restart": what the store saves is
//! what the next launch loads.

use super::NamespaceSets;
use gpui_kit::TestAppContext;

fn fresh_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "fernrohr-namespace-sets-store-{}-{n}.toml",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

#[gpui_kit::test]
fn edits_survive_a_relaunch(cx: &mut TestAppContext) {
    let path = fresh_path();
    cx.update(|cx| {
        NamespaceSets::init(path.clone(), cx);
        assert!(NamespaceSets::get(cx).sets.is_empty(), "first run");
        NamespaceSets::update(cx, |sets| {
            sets.create("team-workloads", vec!["team-a".into()])
        })
        .unwrap();
        NamespaceSets::update(cx, |sets| sets.apply_to("team-workloads", "team-b", true)).unwrap();
        // A refused change saves nothing.
        assert!(NamespaceSets::update(cx, |sets| sets.create("", vec!["x".into()])).is_err());
    });

    // The next launch.
    let relaunch = TestAppContext::single();
    relaunch.update(|cx| {
        NamespaceSets::init(path.clone(), cx);
        let sets = NamespaceSets::get(cx);
        assert_eq!(sets.sets.len(), 1);
        assert_eq!(
            sets.find("team-workloads").unwrap().namespaces,
            ["team-a", "team-b"]
        );
    });
    let _ = std::fs::remove_file(path);
}
