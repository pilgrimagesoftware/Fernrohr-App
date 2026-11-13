//! Drawing the Resources tab (helm-releases section 6): one row per object,
//! its name a `resource-links` reference, the selected row highlighted, a
//! missing object marked. What the rows are is `resources`'.

use super::super::panel::HelmReleasePanel;
use super::{Presence, Resources};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The note under a list missing objects from documents that couldn't be
/// read.
pub(crate) const UNREADABLE_NOTE: &str = "helm-release-resources-unreadable";

impl HelmReleasePanel {
    /// The deployed revision's objects, one row each: kind, its name as a
    /// `resource-links` reference, its namespace, and whether it's missing.
    /// The selected row is what Enter follows; a click selects it, and a
    /// click on its name follows it.
    pub(in crate::k8s::resource::helm::detail) fn resources_tab(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui::typography::TypeRole as _;
        let rows = match &self.resources {
            Resources::NotFetched | Resources::Loading { .. } => {
                return div().child("\u{2026}").into_any_element();
            }
            Resources::Unreadable { reason, .. } => {
                return div()
                    .text_color(crate::ui::style::status(crate::ui::style::Tone::Bad, cx))
                    .child(format!("The deployed revision couldn't be read: {reason}"))
                    .into_any_element();
            }
            Resources::Ready {
                rows, unreadable, ..
            } if rows.is_empty() && *unreadable == 0 => {
                return div()
                    .child("The deployed revision's manifest declares no objects.")
                    .into_any_element();
            }
            Resources::Ready {
                rows, unreadable, ..
            } => (rows.clone(), *unreadable),
        };
        let (rows, unreadable) = rows;
        let kinds = self.discovery.read(cx).kinds().map(<[_]>::to_vec);
        let mut list = div().flex().flex_col().gap_0p5();
        for (ix, row) in rows.into_iter().enumerate() {
            let this = cx.weak_entity();
            let id = format!("helm-release-resource-{ix}");
            let selector = id.clone();
            let missing = row.presence == Presence::Missing;
            let link = crate::ui::link::references(
                format!("helm-release-resource-link-{ix}"),
                std::slice::from_ref(&row.target),
                |target| target.name.clone(),
                &self.scope.context_name,
                kinds.as_deref(),
                cx,
            );
            list = list.child(
                div()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .gap_3()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .data_font()
                    .when(self.selected_resource == ix, |row| {
                        row.bg(crate::ui::style::accent_subtle(cx))
                    })
                    .on_click(move |_, _window, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.selected_resource = ix;
                            cx.notify();
                        });
                    })
                    .child(div().w(px(180.)).child(row.target.kind.clone()))
                    .child(div().w(px(260.)).child(link))
                    .child(
                        div()
                            .w(px(160.))
                            .child(row.target.namespace.clone().unwrap_or_default()),
                    )
                    .when(missing, |row| {
                        row.child(
                            div()
                                .text_color(crate::ui::style::status(
                                    crate::ui::style::Tone::Bad,
                                    cx,
                                ))
                                .child("missing"),
                        )
                    }),
            );
        }
        // The list may be short: say so rather than let it pass for whole.
        if unreadable > 0 {
            let documents = if unreadable == 1 {
                "1 document".to_string()
            } else {
                format!("{unreadable} documents")
            };
            list = list.child(
                div()
                    .debug_selector(|| UNREADABLE_NOTE.into())
                    .px_2()
                    .pt_1()
                    .text_color(crate::ui::style::status(
                        crate::ui::style::Tone::Warning,
                        cx,
                    ))
                    .child(format!(
                        "{documents} in the manifest couldn't be read, so this list may be \
                         incomplete."
                    )),
            );
        }
        list.into_any_element()
    }
}
