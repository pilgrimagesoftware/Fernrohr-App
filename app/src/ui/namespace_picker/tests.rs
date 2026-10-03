//! `namespace-picker-filter` 2.1-2.3, by real keystrokes and clicks in a
//! window: a host view with a namespaced scope and the picker, Pick Namespaces
//! bound through the command registry the way each panel binds it.

use super::{NamespacePickerSlot, PickNamespaces, label_for, pick_namespaces_command};
use crate::command::CommandRegistry;
use crate::keymap::{self, KeymapConfig};
use crate::ui::namespace_filter::{NO_MATCH_SELECTOR, checked_selector, option_selector};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Pixels, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    WindowHandle, div, px,
};

const CONTEXT: &str = "PickerHost";

const NAMESPACES: [&str; 4] = ["default", "kube-public", "kube-system", "payments"];

/// A panel stand-in: a namespaced scope, re-scoped by its picker.
struct Host {
    scope: PanelScope,
    namespaces: Vec<String>,
    slot: NamespacePickerSlot,
    focus_handle: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let namespaces = self.namespaces.clone();
        let picker = self.slot.element(
            &self.scope,
            &namespaces,
            move |namespaces, cx| {
                let _ = this.update(cx, |host: &mut Self, cx| {
                    host.scope = host.scope.scoped_to(namespaces);
                    cx.notify();
                });
            },
            window,
            cx,
        );
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context(CONTEXT)
            .on_action(
                cx.listener(|host, _: &PickNamespaces, window, cx| host.slot.open(window, cx)),
            )
            .flex()
            .justify_end()
            .children(picker)
    }
}

struct Harness {
    host: Entity<Host>,
    window: WindowHandle<Root>,
}

fn harness(cx: &mut TestAppContext) -> (Harness, VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        registry.register(pick_namespaces_command(
            "host.pick_namespaces",
            "Host: Pick Namespaces",
            CONTEXT,
        ));
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            scope: PanelScope::new(NavTarget::pods(), "kind-dev".into()),
            namespaces: NAMESPACES.map(String::from).to_vec(),
            slot: NamespacePickerSlot::default(),
            focus_handle: cx.focus_handle(),
        });
        built = Some(host.clone());
        Root::new(host, window, cx)
    });
    let host = built.expect("the window built its host");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let focus = host.read_with(&vcx, |host, _| host.focus_handle.clone());
    vcx.update(|window, cx| focus.focus(window, cx));
    vcx.run_until_parked();
    (Harness { host, window }, vcx)
}

fn drawn(vcx: &mut VisualTestContext, h: &Harness, selector: String) -> bool {
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector.leak()).is_some()
}

/// The entries the open picker lists, in the order they're drawn.
fn listed(vcx: &mut VisualTestContext, h: &Harness) -> Vec<String> {
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    let mut rows: Vec<(Pixels, String)> = std::iter::once(None)
        .chain(NAMESPACES.map(Some))
        .filter_map(|entry| {
            let bounds = vcx.debug_bounds(option_selector(entry).leak())?;
            Some((
                bounds.top(),
                entry
                    .unwrap_or(super::super::namespace_filter::ALL_NAMESPACES)
                    .to_string(),
            ))
        })
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("drawn rows have positions"));
    rows.into_iter().map(|(_, name)| name).collect()
}

fn is_open(vcx: &mut VisualTestContext, h: &Harness) -> bool {
    h.host.read_with(vcx, |host, cx| {
        host.slot
            .picker()
            .is_some_and(|picker| picker.read(cx).is_open())
    })
}

fn query(vcx: &mut VisualTestContext, h: &Harness) -> String {
    h.host.read_with(vcx, |host, cx| {
        host.slot
            .picker()
            .map(|picker| picker.read(cx).filter.query(cx).to_string())
            .unwrap_or_default()
    })
}

fn scope(vcx: &mut VisualTestContext, h: &Harness) -> Vec<String> {
    h.host
        .read_with(vcx, |host, _| host.scope.namespaces.clone())
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    vcx.run_until_parked();
}

fn type_text(vcx: &mut VisualTestContext, text: &str) {
    vcx.simulate_input(text);
    vcx.run_until_parked();
}

fn click(vcx: &mut VisualTestContext, h: &Harness, selector: String) {
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    let bounds = vcx
        .debug_bounds(selector.clone().leak())
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    vcx.simulate_click(bounds.center(), Modifiers::none());
    vcx.run_until_parked();
}

fn everything() -> Vec<String> {
    [
        "All namespaces",
        "default",
        "kube-public",
        "kube-system",
        "payments",
    ]
    .map(String::from)
    .to_vec()
}

/// Spec: "Typing narrows the list" - `n` opens the picker with focus in the
/// filter, and `KUBE` leaves "All namespaces" and the `kube-*` names.
#[gpui_kit::test]
async fn typing_narrows_the_list(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    assert!(is_open(&mut vcx, &h), "`n` opens the picker");
    assert_eq!(listed(&mut vcx, &h), everything());

    type_text(&mut vcx, "KUBE");
    assert_eq!(
        listed(&mut vcx, &h),
        ["All namespaces", "kube-public", "kube-system"]
    );
}

/// Spec: "Picking several matches in a row" - each toggle keeps the picker
/// open on its filter, and the scope gathers both.
#[gpui_kit::test]
async fn picking_several_matches_in_a_row(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    type_text(&mut vcx, "kube");

    click(&mut vcx, &h, option_selector(Some("kube-system")));
    assert!(is_open(&mut vcx, &h), "a toggle leaves the picker open");
    assert_eq!(query(&mut vcx, &h), "kube", "and its filter as typed");
    assert_eq!(scope(&mut vcx, &h), ["kube-system"]);
    // The click left the highlight on `kube-system`, the last row; Up reaches
    // `kube-public`, and Enter picks it too.
    press(&mut vcx, "up enter");
    assert_eq!(scope(&mut vcx, &h), ["kube-public", "kube-system"]);
    assert!(is_open(&mut vcx, &h));
    assert_eq!(query(&mut vcx, &h), "kube");
    assert_eq!(label_for(&scope(&mut vcx, &h)), "2 namespaces");
    assert!(drawn(&mut vcx, &h, checked_selector(Some("kube-public"))));
    assert!(drawn(&mut vcx, &h, checked_selector(Some("kube-system"))));
}

/// Spec: "Keyboard toggle" - Down from the pinned entry highlights the one
/// match, Enter toggles it.
#[gpui_kit::test]
async fn down_and_enter_toggle_the_highlighted_namespace(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    type_text(&mut vcx, "pay");
    press(&mut vcx, "down enter");
    assert_eq!(scope(&mut vcx, &h), ["payments"]);
    press(&mut vcx, "enter");
    assert!(scope(&mut vcx, &h).is_empty(), "Enter again toggles it off");
}

/// Spec: "Escape clears, then closes".
#[gpui_kit::test]
async fn escape_clears_then_closes(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    type_text(&mut vcx, "kube");
    press(&mut vcx, "escape");
    assert!(is_open(&mut vcx, &h), "the first Escape only clears");
    assert_eq!(query(&mut vcx, &h), "");
    assert_eq!(listed(&mut vcx, &h), everything());
    press(&mut vcx, "escape");
    assert!(!is_open(&mut vcx, &h), "the second closes");
}

/// Spec: "No match".
#[gpui_kit::test]
async fn a_filter_matching_nothing_says_so(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    assert!(!drawn(&mut vcx, &h, NO_MATCH_SELECTOR.to_string()));
    type_text(&mut vcx, "zzz");
    assert_eq!(listed(&mut vcx, &h), ["All namespaces"]);
    assert!(drawn(&mut vcx, &h, NO_MATCH_SELECTOR.to_string()));
}

/// Spec: "Filter resets on reopen".
#[gpui_kit::test]
async fn the_filter_resets_on_reopen(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    type_text(&mut vcx, "kube");
    // A click outside closes it with the text still typed - unlike Escape,
    // which clears the text before it closes.
    vcx.simulate_click(gpui_kit::point(px(10.), px(400.)), Modifiers::none());
    vcx.run_until_parked();
    assert!(!is_open(&mut vcx, &h), "a click outside closes the picker");
    assert_eq!(query(&mut vcx, &h), "kube");
    let focus = h.host.read_with(&vcx, |host, _| host.focus_handle.clone());
    vcx.update(|window, cx| focus.focus(window, cx));
    press(&mut vcx, "n");
    assert_eq!(query(&mut vcx, &h), "");
    assert_eq!(listed(&mut vcx, &h), everything());
}

/// The mouse route: the button opens the picker, a click on a row toggles it,
/// and "All namespaces" clears the scope.
#[gpui_kit::test]
async fn the_button_and_a_click_pick_too(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    let button = vcx
        .update_window(h.window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(gpui_kit::ElementId::Name("panel-namespace".into()))
                .expect("the button is drawn")
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(button, Modifiers::none());
    vcx.run_until_parked();
    assert!(is_open(&mut vcx, &h), "a click opens the picker");

    click(&mut vcx, &h, option_selector(Some("default")));
    assert_eq!(scope(&mut vcx, &h), ["default"]);
    click(&mut vcx, &h, option_selector(None));
    assert!(scope(&mut vcx, &h).is_empty());
    assert!(drawn(&mut vcx, &h, checked_selector(None)));
}

/// 2.3: a scope changed while the picker is closed - a warp command - is what
/// it shows checked on the next open.
#[gpui_kit::test]
async fn a_scope_changed_elsewhere_shows_on_the_next_open(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    press(&mut vcx, "escape");
    h.host.update(&mut vcx, |host, cx| {
        host.scope = host.scope.scoped_to(vec!["payments".to_string()]);
        cx.notify();
    });
    vcx.run_until_parked();
    press(&mut vcx, "n");
    assert!(drawn(&mut vcx, &h, checked_selector(Some("payments"))));
    assert!(!drawn(&mut vcx, &h, checked_selector(None)));
}

/// Every panel with a picker offers Pick Namespaces, on `n`, while it has
/// focus - so the palette lists it there and the keymap can rebind it.
#[test]
fn each_panel_with_a_picker_registers_pick_namespaces() {
    let mut registry = CommandRegistry::new();
    crate::k8s::resource::pods::register_commands(&mut registry);
    crate::k8s::resource::object_list::register_commands(&mut registry);
    crate::k8s::resource::events_browser::register_commands(&mut registry);
    crate::ui::placeholder::register_commands(&mut registry);
    for context in [
        crate::k8s::resource::pods::PANEL_KEY_CONTEXT,
        "ObjectListPanel",
        "EventsPanel",
        crate::ui::placeholder::PANEL_KEY_CONTEXT,
    ] {
        let pick: Vec<_> = registry
            .available(&[context])
            .into_iter()
            .filter(|command| command.action.partial_eq(&PickNamespaces))
            .collect();
        assert_eq!(pick.len(), 1, "{context} offers Pick Namespaces once");
        assert_eq!(pick[0].default_binding, "n");
        assert!(
            pick[0].menu.is_none(),
            "a panel command stays out of the menu bar"
        );
    }
}

/// Hover never moves the selection: with the pointer resting on one row,
/// Enter still toggles the row the keyboard chose.
#[gpui_kit::test]
async fn enter_toggles_the_keyboard_row_not_the_hovered_one(cx: &mut TestAppContext) {
    let (h, mut vcx) = harness(cx);
    press(&mut vcx, "n");
    type_text(&mut vcx, "pay");
    press(&mut vcx, "down");
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    let resting = vcx
        .debug_bounds(option_selector(None).leak())
        .expect("All namespaces is drawn");
    vcx.simulate_mouse_move(resting.center(), None, Modifiers::none());
    vcx.run_until_parked();
    press(&mut vcx, "enter");
    assert_eq!(
        scope(&mut vcx, &h),
        ["payments"],
        "the keyboard's row, not the hovered All namespaces"
    );
}
