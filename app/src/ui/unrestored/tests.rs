//! `restore-no-panic`: every panel kind's restore, fed state it can't use,
//! stands an [`UnrestoredPanel`] in rather than panicking - and that panel saves
//! the original state back unchanged.

use super::UnrestoredPanel;
use gpui_kit::component::dock::{DockAreaState, DockPlacement, DockSkin, PanelInfo, PanelState};
use gpui_kit::{Entity, TestAppContext};
use serde_json::json;
use std::rc::Rc;

struct DockHost {
    area: Entity<gpui_kit::component::dock::DockArea>,
    _skin: Rc<DockSkin>,
}

impl gpui_kit::Render for DockHost {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{ParentElement as _, Styled as _};
        gpui_kit::div().size_full().child(self.area.clone())
    }
}

fn leaf(panel_name: &str, info: PanelInfo) -> PanelState {
    PanelState {
        panel_name: panel_name.to_string(),
        children: Vec::new(),
        info,
    }
}

/// For each registered panel kind, a saved state its restore can't read: a
/// state missing what it names its panel by, and one whose info isn't a
/// panel's at all.
fn malformed() -> Vec<PanelState> {
    let not_a_panel = || PanelInfo::Stack {
        sizes: Vec::new(),
        axis: 0,
    };
    let mut states = Vec::new();
    for kind in [
        "Pods",
        "Logs",
        "PodDetail",
        "ObjectDetail",
        "ObjectList",
        "Resource",
    ] {
        states.push(leaf(kind, PanelInfo::Panel(json!({}))));
        states.push(leaf(kind, not_a_panel()));
    }
    // Present but unusable fields: a cluster with no pod, an object list with
    // no kind.
    states.push(leaf(
        "PodDetail",
        PanelInfo::Panel(json!({ "context_name": "kind-dev" })),
    ));
    states.push(leaf(
        "ObjectList",
        PanelInfo::Panel(json!({ "context_name": "kind-dev", "group": "" })),
    ));
    states
}

#[gpui_kit::test]
fn every_kinds_malformed_state_restores_as_a_placeholder(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::k8s::resource::pods::register_restore(cx);
        crate::k8s::resource::pod_detail::register_restore(cx);
        crate::k8s::resource::object_detail::register_restore(cx);
        crate::util::logs::register_restore(cx);
        crate::k8s::resource::object_list::register_restore(cx);
        crate::ui::placeholder::register_restore(cx);
        crate::k8s::resource::events_browser::register_restore(cx);
    });
    let (host, cx) = cx.add_window_view(|window, cx| {
        let (area, skin) = DockSkin::dock_area("restore-test", Some(1), window, cx);
        DockHost { area, _skin: skin }
    });
    let area = cx.update(|_, cx| host.read(cx).area.clone());
    let states = malformed();
    let center = PanelState {
        panel_name: "StackPanel".to_string(),
        children: vec![PanelState {
            panel_name: "TabPanel".to_string(),
            children: states.clone(),
            info: PanelInfo::Tabs { active_index: 0 },
        }],
        info: PanelInfo::Stack {
            sizes: Vec::new(),
            axis: 0,
        },
    };

    cx.update(|window, cx| {
        area.update(cx, |area, cx| {
            area.load(
                DockAreaState {
                    center,
                    ..Default::default()
                },
                window,
                cx,
            )
        })
    })
    .expect("the layout loads");
    // Runs the frame that draws the placeholders, which must not panic either.
    cx.run_until_parked();

    let restored: Vec<(String, String, PanelState)> = cx.update(|_, cx| {
        let area = area.read(cx);
        area.layout(DockPlacement::Center)
            .expect("a centre")
            .panels()
            .map(|id| {
                let view = area.panel(id).expect("a built panel");
                let panel: Entity<UnrestoredPanel> = Entity::from(view.as_ref());
                let panel = panel.read(cx);
                (
                    panel.panel_kind().to_string(),
                    panel.reason().to_string(),
                    view.dump(cx),
                )
            })
            .collect()
    });
    assert_eq!(
        restored.len(),
        states.len(),
        "every panel restored as something"
    );
    for ((kind, reason, dumped), original) in restored.iter().zip(&states) {
        assert_eq!(kind, &original.panel_name);
        assert!(!reason.is_empty(), "{kind} says why");
        assert_eq!(dumped, original, "{kind} saves its state back unchanged");
    }
}
