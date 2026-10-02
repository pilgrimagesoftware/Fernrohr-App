//! Asserting an inline kind icon in a rendered window, for the views that
//! draw one.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Bounds, ElementId, Pixels, VisualTestContext, px};

/// The bounds of the icon `selector` names, after a fresh frame. `None` if no
/// such icon is drawn.
pub(crate) fn icon_bounds(vcx: &mut VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    vcx.update(|window, cx| window.render_frame(cx));
    // `debug_bounds` wants a `'static` selector; a test's few are leaked.
    vcx.debug_bounds(selector.leak())
}

/// `selector`'s icon is drawn, non-empty, and leads `element`: it starts at the
/// element's left edge and ends before the element does, so it sits before the
/// element's text.
pub(crate) fn assert_icon_leads(vcx: &mut VisualTestContext, selector: String, element: ElementId) {
    let icon = icon_bounds(vcx, selector.clone()).unwrap_or_else(|| panic!("{selector} is drawn"));
    let target = vcx
        .update(|window, _| window.try_find(element.clone()).map(|found| found.bounds()))
        .unwrap_or_else(|| panic!("{element:?} is drawn"));
    assert!(icon.size.height > px(0.), "{selector} has a size: {icon:?}");
    assert!(
        (icon.left() - target.left()).abs() < px(0.5) && icon.right() < target.right(),
        "{selector} leads {element:?}: {icon:?} in {target:?}"
    );
}
