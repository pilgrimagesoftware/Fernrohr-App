//! #295: the form sits at the top of the window, each part one control gap
//! below the last, whichever type is chosen - the window's spare height stays
//! above the footer, which stays at the window's foot, rather than opening a gap
//! under the type.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px};

use super::super::body::IssueKind;
use super::super::render::{FOOTER, SUBJECT_LABEL, kind_selector_of};
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
    }
}
