//! Web addresses a detail view shows as links (#157) - an Ingress's hosts.
//! Each is a tab stop that opens its address in the browser on a click, or
//! on Enter or Space once Tab has reached it.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// What a link's tooltip says it does.
pub const OPEN_URL_TOOLTIP: &str = "Open in browser";

/// The link of `id_prefix`'s address `index`.
pub fn url_link_id(id_prefix: &str, index: usize) -> ElementId {
    ElementId::Name(format!("url {id_prefix} {index}").into())
}

/// One link per address in `urls`, stacked.
pub fn urls(id_prefix: &str, urls: &[String]) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_start()
        .gap_1()
        .children(urls.iter().enumerate().map(|(index, url)| {
            let address = url.clone();
            Button::new(url_link_id(id_prefix, index))
                .label(url.clone())
                .link()
                .small()
                .tooltip(OPEN_URL_TOOLTIP)
                .on_click(move |_, _, cx| cx.open_url(&address))
        }))
        .into_any_element()
}
