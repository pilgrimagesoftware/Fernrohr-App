//! #295: the form's parts stack one control gap apart, whichever type is
//! chosen. Only Description grows, taking the window's spare height, so no gap
//! opens under the type and the buttons stay at the window's foot.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Focusable as _, Modifiers, TestAppContext, point, px};

use super::super::body::IssueKind;
use super::super::render::{
    DESCRIPTION, DESCRIPTION_MIN_HEIGHT, FOOTER, ROUTE_LINE, SUBJECT_LABEL, kind_selector_of,
};
use super::{open, setup};

#[gpui_kit::test]
async fn subject_follows_the_type_directly_for_either_type(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    let view = opened.view.clone();
    let gap = opened
        .vcx
        .update(|_, cx| crate::ui::space::spacing(cx).control_gap);
    for kind in IssueKind::ALL {
        opened.vcx.update(|window, cx| {
            view.update(cx, |this, cx| this.set_kind(kind, window, cx));
        });
        opened.vcx.run_until_parked();
        opened.vcx.update(|window, cx| window.render_frame(cx));
        let selector: &'static str = kind_selector_of(kind).leak();
        let kinds = opened
            .vcx
            .debug_bounds(selector)
            .expect("the type is drawn");
        let subject = opened
            .vcx
            .debug_bounds(SUBJECT_LABEL)
            .expect("the Subject label is drawn");
        let space = subject.top() - kinds.bottom();
        assert!(
            space <= gap + px(1.),
            "{kind:?}: Subject is {space:?} below the type, not one gap ({gap:?})"
        );
        let footer = opened
            .vcx
            .debug_bounds(FOOTER)
            .expect("the footer is drawn");
        let (height, inset) = opened.vcx.update(|window, cx| {
            (
                window.viewport_size().height,
                crate::ui::space::spacing(cx).panel_inset,
            )
        });
        assert!(
            (footer.bottom() - (height - inset)).abs() <= px(1.),
            "{kind:?}: the footer ends at {:?}, not the window's foot ({height:?} less {inset:?})",
            footer.bottom()
        );
        let route = opened
            .vcx
            .debug_bounds(ROUTE_LINE)
            .expect("the route is drawn");
        let space = footer.top() - route.bottom();
        assert!(
            space <= gap + px(1.),
            "{kind:?}: the footer is {space:?} below the route line, not one gap"
        );
        let description = opened
            .vcx
            .debug_bounds(DESCRIPTION)
            .expect("Description is drawn");
        assert!(
            description.size.height > px(DESCRIPTION_MIN_HEIGHT),
            "{kind:?}: Description takes the spare height: {description:?}"
        );
    }
}

/// The field itself fills the grown area: a click near its foot, well below
/// the field's minimum height, lands in it.
#[gpui_kit::test]
async fn the_description_field_fills_its_grown_area(cx: &mut TestAppContext) {
    setup(cx);
    let mut opened = open(cx, None);
    let view = opened.view.clone();
    let description = opened
        .vcx
        .debug_bounds(DESCRIPTION)
        .expect("Description is drawn");
    assert!(description.size.height > px(DESCRIPTION_MIN_HEIGHT + 40.));
    let foot = point(description.left() + px(20.), description.bottom() - px(8.));
    opened.vcx.simulate_click(foot, Modifiers::none());
    opened.vcx.run_until_parked();
    let focused = opened.vcx.update(|window, cx| {
        view.read(cx)
            .description
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(
        focused,
        "the click at {foot:?} reaches the field: {description:?}"
    );
}
