// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::command::CommandRegistry;
use crate::util::shell::{ToggleCommandPalette, WindowLayout, open_window, register_commands};
use gpui_kit::TestAppContext;
use gpui_kit::component::WindowExt as _;

#[gpui_kit::test]
async fn toggle_command_palette_action_opens_a_dialog(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        cx.set_global(registry);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    let window = cx.update(|cx| cx.windows()[0]);

    // `has_active_dialog` reads the Root's dialog state without drawing the
    // dialog, so it is leak-safe on both sides of the dispatch.
    let dialog_open = |cx: &mut TestAppContext| {
        window
            .update(cx, |_, window, cx| window.has_active_dialog(cx))
            .unwrap()
    };
    assert!(!dialog_open(cx));

    window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(ToggleCommandPalette), cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert!(dialog_open(cx), "the palette opened as a dialog");

    // Close the dialog before the test ends, or the leak detector flags
    // its CommandState entity: the harness asserts every entity created
    // during a test is released by teardown.
    window
        .update(cx, |_, window, cx| {
            if matches!(window.root::<gpui_kit::component::Root>(), Some(Some(_))) {
                window.close_all_dialogs(cx);
            }
        })
        .unwrap();
    window
        .update(cx, |_, window, _cx| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}

/// `visual-refresh-typography-spacing` 2.2: a real window's frame - the
/// panels' tabs (a tab's label is its panel's title), the Resource panel's
/// heading, the context bar and the status bar - is drawn in the frame role's
/// family, inherited from the theme through `Root`.
#[test]
fn a_windows_frame_is_drawn_in_the_frame_family() {
    use crate::config::workspace::{NamespaceScope, PanelDescriptor, SortState};
    use crate::ui::typography::FRAME_FAMILY;
    use crate::ui::typography::recorder::with_recorded_text;
    use gpui_kit::test::TestWindowExt as _;

    with_recorded_text(|cx, recorded| {
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        });
        let pods = |namespace: &str| PanelDescriptor::Pods {
            cluster_context: "kind-dev".to_string(),
            namespace: NamespaceScope::Single(namespace.to_string()),
            filter: String::new(),
            sort: SortState {
                column: "name".into(),
                ascending: true,
            },
        };
        // Two panels in one group, so the dock draws a tab bar.
        let layout = WindowLayout {
            contexts: vec!["kind-dev".to_string()],
            panels: vec![pods("default"), pods("kube-system")],
            ..Default::default()
        };
        cx.update(|cx| open_window(cx, layout));
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .unwrap();

        let mut vcx = gpui_kit::VisualTestContext::from_window(window, cx);
        assert!(
            vcx.debug_bounds("panel-title-Pods-unfocused").is_some()
                || vcx.debug_bounds("panel-title-Pods-focused").is_some(),
            "the Pods tabs were drawn"
        );
        for (surface, text) in [
            ("a tab", "Pods"),
            ("the Resource panel's heading", "Resources"),
            ("the context bar", "kind-dev"),
            ("the status bar", "Connected"),
        ] {
            assert_eq!(
                recorded.family_of(text).as_ref(),
                FRAME_FAMILY,
                "{surface} ({text:?}) is frame text"
            );
        }
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
}

/// `visual-refresh-typography-spacing` 3.2, context and status bars: each
/// bar's first item sits at least the panel inset from the window's edge, at
/// 150% text so the inset is the scaled token.
#[gpui_kit::test]
async fn the_bars_items_are_inset_from_the_window_edge(cx: &mut TestAppContext) {
    use crate::ui::space::{TextScale, spacing};
    use gpui_kit::test::TestWindowExt as _;

    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        TextScale::new(1.5).expect("a valid scale").set(cx);
        open_window(
            cx,
            WindowLayout {
                contexts: vec!["kind-dev".to_string()],
                ..Default::default()
            },
        );
    });
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    let (inset, chip) = window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let chip = window
                .try_find(gpui_kit::ElementId::from(gpui_kit::SharedString::from(
                    "context-chip-kind-dev",
                )))
                .expect("the context bar's chip is drawn")
                .bounds();
            (spacing(cx).panel_inset, chip)
        })
        .unwrap();
    assert!(
        chip.left() >= inset,
        "the context chip starts {:?} from the edge, under {inset:?}",
        chip.left()
    );

    let mut vcx = gpui_kit::VisualTestContext::from_window(window, cx);
    let item = vcx
        .debug_bounds("status-item-kind-dev")
        .expect("the status bar's item is drawn");
    assert!(
        item.left() >= inset,
        "the status item starts {:?} from the edge, under {inset:?}",
        item.left()
    );
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}
