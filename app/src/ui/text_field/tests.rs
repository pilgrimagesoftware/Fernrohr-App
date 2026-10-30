//! The clear button, through a real window: hidden while a field is empty,
//! shown once it has text, and a click that empties it, keeps focus and sends
//! the change a filter listens for. Not on a masked field.

use super::text_field;
use gpui_kit::component::Root;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div,
};

/// A window with one field, counting the changes it sends.
struct Field {
    state: Entity<InputState>,
    changes: usize,
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p_4().child(text_field(&self.state, cx))
    }
}

/// The clear button's id, gpui-component's own.
const CLEAR: &str = "clean";

fn open(cx: &mut TestAppContext, masked: bool) -> (Entity<Field>, VisualTestContext) {
    cx.update(crate::util::test_ui::init);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let field = cx.new(|cx| {
            let state = cx.new(|cx| InputState::new(window, cx).masked(masked));
            cx.subscribe(&state, |this: &mut Field, _, event: &InputEvent, _| {
                if matches!(event, InputEvent::Change) {
                    this.changes += 1;
                }
            })
            .detach();
            Field { state, changes: 0 }
        });
        built = Some(field.clone());
        Root::new(field, window, cx)
    });
    let field = built.expect("built");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        field
            .read(cx)
            .state
            .clone()
            .update(cx, |state, cx| state.focus(window, cx));
    });
    vcx.run_until_parked();
    (field, vcx)
}

fn clear_shown(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(CLEAR).is_some()
    })
}

fn text(field: &Entity<Field>, vcx: &mut VisualTestContext) -> String {
    vcx.update(|_, cx| field.read(cx).state.read(cx).value().to_string())
}

#[gpui_kit::test]
async fn clear_shows_only_with_text_and_empties_the_field(cx: &mut TestAppContext) {
    let (field, mut vcx) = open(cx, false);
    assert!(!clear_shown(&mut vcx), "hidden while empty");

    vcx.simulate_input("web");
    vcx.run_until_parked();
    assert!(clear_shown(&mut vcx), "shown once there's text");
    let typed = vcx.update(|_, cx| field.read(cx).changes);

    vcx.update(|window, cx| window.click(CLEAR, cx));
    vcx.run_until_parked();
    assert_eq!(text(&field, &mut vcx), "", "the click empties it");
    assert!(
        vcx.update(|_, cx| field.read(cx).changes) > typed,
        "and sends a change, as typing does"
    );
    let focused = vcx.update(|window, cx| {
        field
            .read(cx)
            .state
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(focused, "focus stays in the field");
    assert!(!clear_shown(&mut vcx), "hidden again, empty");
}

/// A masked field - a secret - gets no button a stray click could wipe it with.
#[gpui_kit::test]
async fn a_masked_field_has_no_clear_button(cx: &mut TestAppContext) {
    let (_, mut vcx) = open(cx, true);
    vcx.simulate_input("secret");
    vcx.run_until_parked();
    assert!(!clear_shown(&mut vcx));
}
