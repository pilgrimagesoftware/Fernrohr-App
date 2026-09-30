//! One reference as an element: a link that dispatches [`FollowReference`]
//! when `ui::viewer` has a panel for its kind, plain text otherwise.

use crate::k8s::object_ref::ObjectRef;
use crate::ui::viewer::viewer_for;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Follow a reference: open (or focus) the referenced object's panel in
/// `context_name`, the cluster context of the panel the reference was shown
/// in - not necessarily the window's active one.
///
/// Carries its data rather than reading a global the way `ShowPodDetail`
/// reads `SelectedPod`: a link is its own selection, and a shared global would
/// race between two panels. Not a registry command either - it means nothing
/// without a specific reference. The user-facing command is `links.go_to`.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = links, no_json)]
pub struct FollowReference {
    pub context_name: String,
    pub target: ObjectRef,
}

/// `target`, shown as `text`: a link when it can be followed, plain unstyled
/// text when it can't - never a link that leads nowhere.
///
/// `id` must be unique among the panel's references; it is what a click
/// targets, and what a test finds the reference by.
fn reference(
    id: impl Into<ElementId>,
    target: &ObjectRef,
    text: impl Into<SharedString>,
    context_name: &str,
    cx: &App,
) -> AnyElement {
    let text = text.into();
    if viewer_for(target).is_none() {
        return div().id(id).child(text).test_support().into_any_element();
    }
    let action = FollowReference {
        context_name: context_name.to_string(),
        target: target.clone(),
    };
    div()
        .id(id)
        .text_color(cx.theme().primary)
        .cursor_pointer()
        .hover(|style| style.underline())
        .on_click(move |_event, window, cx| {
            window.dispatch_action(Box::new(action.clone()), cx);
        })
        .child(text)
        .test_support()
        .into_any_element()
}

/// A run of references, comma-separated, each its own [`reference`] - ids
/// `(id_prefix, index)`. `label` says how each reads (`Kind/name` or the bare
/// name).
pub fn references(
    id_prefix: impl Into<SharedString>,
    targets: &[ObjectRef],
    label: impl Fn(&ObjectRef) -> String,
    context_name: &str,
    cx: &App,
) -> AnyElement {
    let id_prefix = id_prefix.into();
    let last = targets.len().saturating_sub(1);
    div()
        .flex()
        .flex_wrap()
        .gap_x_1()
        .children(targets.iter().enumerate().map(|(index, target)| {
            let link = reference(
                ElementId::NamedInteger(id_prefix.clone(), index as u64),
                target,
                label(target),
                context_name,
                cx,
            );
            div()
                .flex()
                .child(link)
                .when(index < last, |this| this.child(","))
        }))
        .into_any_element()
}
