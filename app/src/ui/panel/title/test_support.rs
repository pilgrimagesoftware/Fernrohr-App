//! Checks the detail panel headers' tests share (`resource-kind-icons` 3.5).

use crate::config::ui::TextSize;
use crate::ui::icon::IconSize;
use gpui_kit::{VisualTestContext, px};

/// The header's kind icon is drawn at the header size - within a pixel and a
/// half under it, the set being a little wider than tall - and leads the
/// resource's name: centred on it, and ending before it starts. Checked at
/// 100% and again at 150% text, in the same window.
pub(crate) fn assert_header_icon_leads_at_every_text_size(vcx: &mut VisualTestContext) {
    for size in [TextSize::DEFAULT, TextSize::MAX] {
        vcx.update(|_, cx| crate::ui::text_size::set(size, cx));
        vcx.run_until_parked();
        let expected = vcx.update(|_, cx| IconSize::Header.logical(cx));
        let icon = vcx
            .debug_bounds("detail-header-icon")
            .expect("the header's kind icon is drawn");
        let name = vcx
            .debug_bounds("item-heading-name")
            .expect("the header's name is drawn");
        let at = size.percent();
        assert!(
            icon.size.height <= expected && icon.size.height >= expected - px(1.5),
            "at {at}% the header icon is {:?} tall, not about {expected:?}",
            icon.size.height
        );
        assert!(
            icon.right() <= name.left(),
            "at {at}% the icon ends at {:?}, after the name starts at {:?}",
            icon.right(),
            name.left()
        );
        assert!(
            (icon.center().y - name.center().y).abs() <= px(1.),
            "at {at}% the icon is centred on the name"
        );
    }
}
