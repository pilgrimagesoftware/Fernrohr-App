//! `namespace-sets` 3.3 and "Sets survive a restart": what the store saves is
//! what the next launch loads.

use super::NamespaceSets;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn edits_survive_a_relaunch(cx: &mut TestAppContext) {
    let path = crate::util::test_paths::temp_path("namespace-sets-store");
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
