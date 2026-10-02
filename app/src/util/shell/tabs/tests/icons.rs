//! `resource-kind-icons` 3.1: each panel's tab leads with its kind's icon, and
//! the icons leave the keyboard route as it was.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::icon::test_hooks::hide_icons;
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, NavTarget, WindowLayout, open_window};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Bounds, FocusHandle, Pixels, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;

fn configmaps() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "ConfigMap"),
        plural: "configmaps".to_string(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A real window (through `open_window`, so `Root` and its Tab navigation are
/// there) with a Pods tab and a ConfigMaps tab in one group.
fn pods_and_configmaps(cx: &mut TestAppContext) -> (AnyWindowHandle, VisualTestContext) {
    use crate::config::workspace::{NamespaceScope, PanelDescriptor, SortState};
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, temp_workspace_path(), &temp_workspace_path());
        open_window(
            cx,
            WindowLayout {
                contexts: vec!["kind-dev".to_string()],
                panels: vec![PanelDescriptor::Pods {
                    cluster_context: "kind-dev".to_string(),
                    namespace: NamespaceScope::All,
                    filter: String::new(),
                    sort: SortState {
                        column: "name".into(),
                        ascending: true,
                    },
                }],
                ..Default::default()
            },
        );
    });
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    let mut vcx = VisualTestContext::from_window(window, cx);
    let main_window = vcx.update(|window, cx| {
        let root = window.root::<Root>().flatten().expect("a Root window");
        root.read(cx)
            .view()
            .clone()
            .downcast::<MainWindow>()
            .unwrap()
    });
    vcx.update(|window, cx| {
        main_window.update(cx, |main_window, cx| {
            main_window.open_target(NavTarget::Kind(configmaps()), window, cx);
        });
    });
    vcx.run_until_parked();
    (window, vcx)
}

/// A tab title's bounds, whichever focus state it was drawn in.
fn title(vcx: &mut VisualTestContext, text: &str) -> Bounds<Pixels> {
    ["focused", "unfocused"]
        .into_iter()
        .find_map(|state| {
            let selector: &'static str = format!("panel-title-{text}-{state}").leak();
            vcx.debug_bounds(selector)
        })
        .unwrap_or_else(|| panic!("the {text} tab is drawn"))
}

/// Each tab's icon is the first thing in its title, inside the title's bounds
/// and left of its text.
#[gpui_kit::test]
async fn the_pods_and_configmaps_tabs_each_lead_with_their_icon(cx: &mut TestAppContext) {
    let (window, mut vcx) = pods_and_configmaps(cx);
    window
        .update(&mut vcx, |_, window, cx| window.render_frame(cx))
        .unwrap();
    for (tab, icon) in [("Pods", "Pod"), ("Configmaps", "ConfigMap")] {
        let title = title(&mut vcx, tab);
        let selector: &'static str = format!("kind-icon-{icon}").leak();
        let icon_bounds = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("the {icon} icon is drawn"));
        assert!(
            (icon_bounds.left() - title.left()).abs() < gpui_kit::px(1.),
            "the {icon} icon starts the {tab} title"
        );
        assert!(
            icon_bounds.right() < title.right(),
            "the {tab} title's text follows its icon"
        );
    }
}

/// The focus each of `presses` Tab keystrokes lands on, starting from `start`.
fn tab_route(
    window: AnyWindowHandle,
    vcx: &mut VisualTestContext,
    start: &FocusHandle,
    presses: usize,
) -> Vec<Option<FocusHandle>> {
    window
        .update(vcx, |_, window, cx| start.focus(window, cx))
        .unwrap();
    vcx.run_until_parked();
    (0..presses)
        .map(|_| {
            press(vcx, "tab");
            window
                .update(vcx, |_, window, cx| window.focused(cx))
                .unwrap()
        })
        .collect()
}

/// Tabbing through a panel with icons visits the same controls in the same
/// order as the same panel with them hidden: icons are never tab stops. Tab
/// stays inside the focused panel, so this starts in the Resource panel - its
/// rows show kind icons, and it has controls to tab between whether or not the
/// cluster is reachable.
#[gpui_kit::test]
async fn icons_leave_the_tab_order_unchanged(cx: &mut TestAppContext) {
    let (window, mut vcx) = pods_and_configmaps(cx);
    let start = vcx.update(|window, cx| {
        let root = window.root::<Root>().flatten().expect("a Root window");
        let main_window = root
            .read(cx)
            .view()
            .clone()
            .downcast::<MainWindow>()
            .unwrap();
        main_window
            .read(cx)
            .test_resource_panel()
            .expect("a workspace window")
            .read(cx)
            .focus_handle()
    });
    const PRESSES: usize = 12;

    let with_icons = tab_route(window, &mut vcx, &start, PRESSES);
    let without_icons = {
        let _hidden = hide_icons();
        window
            .update(&mut vcx, |_, window, _| window.refresh())
            .unwrap();
        tab_route(window, &mut vcx, &start, PRESSES)
    };
    let mut distinct: Vec<&FocusHandle> = Vec::new();
    for handle in with_icons.iter().flatten() {
        if !distinct.contains(&handle) {
            distinct.push(handle);
        }
    }
    assert!(
        distinct.len() > 1,
        "Tab moves focus between controls at all"
    );
    assert_eq!(with_icons, without_icons);
}
