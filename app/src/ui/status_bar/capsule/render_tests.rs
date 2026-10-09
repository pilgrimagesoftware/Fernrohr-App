//! What a capsule draws (`status-capsule-icons` 1.1-1.2): `<context> [<tunnel>]
//! <state icon>`, then the elapsed time unless connected - the context name in
//! the frame font, the tunnel and elapsed time in the data font - with the state
//! named only in the icon's tooltip.
//!
//! Parts are found by their debug selectors, laid out in a real window; fonts
//! through the recording text system; tooltips by hovering the icon.
//!
//! Not `use super::*;` - see the comment above `mod tests;` in `status_bar.rs`.

use super::{elapsed_selector, icon_selector, name_selector, tunnel_selector};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::watch_registry::PauseReason;
use crate::ui::picker_tunnel::TunnelChoice;
use crate::ui::status_bar::{Clock, StatusBarView};
use crate::ui::typography::{DATA_FAMILY, FRAME_FAMILY};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Modifiers, Pixels, TestAppContext, VisualTestContext, WeakEntity,
};
use kube::{Client, Config};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How a test sets up `context`'s health before the bar is built.
enum State {
    /// Connected, as a session that is still connecting reports.
    Connected,
    Reconnecting,
    Failed(&'static str),
}

fn init(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        crate::runtime::init(cx);
    });
}

fn test_client(cx: &mut TestAppContext) -> Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

fn set_state(cx: &mut TestAppContext, context: &str, state: State) {
    match state {
        State::Connected => cx.update(|cx| {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }),
        State::Reconnecting => {
            let client = test_client(cx);
            cx.update(|cx| {
                ClusterRegistry::insert_test_session(
                    cx,
                    context,
                    ConnectionState::Connected(client.clone()),
                );
                ClusterRegistry::subscribe_pods(cx, context, client);
                ClusterRegistry::apply_health_transition(
                    cx,
                    context,
                    HealthTransition::Pause(PauseReason::Reconnecting),
                );
            });
        }
        State::Failed(reason) => cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                context,
                ConnectionState::Failed(reason.to_string()),
            );
        }),
    }
}

/// A window drawing a bar for `context`, bound to the tunnel named `tunnel`
/// if any, on a clock that starts now and `advance` later.
fn bar_window(
    cx: &mut TestAppContext,
    context: &str,
    tunnel: Option<&str>,
    advance: Duration,
) -> VisualTestContext {
    let clock = Rc::new(Cell::new(Instant::now() + advance));
    let names = vec![context.to_string()];
    let binding = tunnel.map(|name| (context.to_string(), name.to_string()));
    let window = cx.add_window(move |window, cx| {
        let bar = cx.new(|cx| {
            let mut bar = StatusBarView::new_with_clock(
                names,
                WeakEntity::new_invalid(),
                Clock::Fake(clock),
                cx,
            );
            bar.tunnels_path = crate::util::test_paths::temp_path("status-capsule-render");
            if let Some((context, name)) = binding {
                bar.tunnel_choices = vec![TunnelChoice {
                    id: "tunnel".to_string(),
                    name,
                }];
                bar.tunnel_bindings = BTreeMap::from([(context, "tunnel".to_string())]);
            }
            bar
        });
        Root::new(bar, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    vcx
}

/// Where `selector` was drawn, if it was.
fn drawn(vcx: &mut VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds(selector.leak())
}

fn at(vcx: &mut VisualTestContext, selector: String) -> Bounds<Pixels> {
    drawn(vcx, selector.clone()).unwrap_or_else(|| panic!("{selector} was not drawn"))
}

/// Whether hovering `context`'s state icon shows a tooltip reading `text`.
fn tooltip_reads(vcx: &mut VisualTestContext, context: &str, text: &str) -> bool {
    let center = at(vcx, icon_selector(context)).center();
    vcx.simulate_mouse_move(center, None, Modifiers::none());
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    drawn(vcx, crate::ui::icon_tooltip::selector(text)).is_some()
}

#[test]
fn a_bound_connected_capsule_reads_context_tunnel_icon() {
    crate::ui::typography::recorder::with_recorded_text(|cx, recorded| {
        init(cx);
        set_state(cx, "staging", State::Connected);
        let mut vcx = bar_window(cx, "staging", Some("qa-bastion"), Duration::ZERO);

        let name = at(&mut vcx, name_selector("staging"));
        let tunnel = at(&mut vcx, tunnel_selector("staging"));
        let icon = at(&mut vcx, icon_selector("staging"));
        assert!(name.right() <= tunnel.left(), "the tunnel follows the name");
        assert!(tunnel.right() <= icon.left(), "the icon follows the tunnel");
        assert!(
            drawn(&mut vcx, elapsed_selector("staging")).is_none(),
            "a connected capsule shows no elapsed time"
        );

        assert_eq!(recorded.family_of("staging").as_ref(), FRAME_FAMILY);
        assert_eq!(
            recorded.family_of("[qa-bastion]").as_ref(),
            DATA_FAMILY,
            "the bracketed tunnel is data text, brackets included"
        );
        assert!(
            recorded.families_of("Connected").is_none(),
            "the state text is in the tooltip, not the capsule"
        );
    });
}

#[gpui_kit::test]
async fn an_unbound_capsule_reads_context_then_icon(cx: &mut TestAppContext) {
    init(cx);
    set_state(cx, "kind-dev", State::Connected);
    let mut vcx = bar_window(cx, "kind-dev", None, Duration::ZERO);

    let name = at(&mut vcx, name_selector("kind-dev"));
    let icon = at(&mut vcx, icon_selector("kind-dev"));
    assert!(name.right() <= icon.left());
    assert!(drawn(&mut vcx, tunnel_selector("kind-dev")).is_none());
    assert!(drawn(&mut vcx, elapsed_selector("kind-dev")).is_none());
}

#[test]
fn a_reconnecting_capsule_counts_up_after_its_icon() {
    crate::ui::typography::recorder::with_recorded_text(|cx, recorded| {
        init(cx);
        set_state(cx, "staging", State::Reconnecting);
        let mut vcx = bar_window(cx, "staging", Some("qa-bastion"), Duration::from_secs(42));

        let icon = at(&mut vcx, icon_selector("staging"));
        let elapsed = at(&mut vcx, elapsed_selector("staging"));
        assert!(
            icon.right() <= elapsed.left(),
            "the elapsed time follows the icon"
        );
        assert_eq!(
            recorded.family_of("42s").as_ref(),
            DATA_FAMILY,
            "the elapsed time is data text"
        );
        assert!(recorded.families_of("Reconnecting").is_none());
    });
}

#[gpui_kit::test]
async fn a_connected_icon_is_named_in_its_tooltip(cx: &mut TestAppContext) {
    init(cx);
    set_state(cx, "kind-dev", State::Connected);
    let mut vcx = bar_window(cx, "kind-dev", None, Duration::ZERO);
    assert!(tooltip_reads(&mut vcx, "kind-dev", "Connected"));
}

#[gpui_kit::test]
async fn a_reconnecting_icons_tooltip_says_for_how_long(cx: &mut TestAppContext) {
    init(cx);
    set_state(cx, "kind-dev", State::Reconnecting);
    let mut vcx = bar_window(cx, "kind-dev", None, Duration::from_secs(42));
    assert!(tooltip_reads(&mut vcx, "kind-dev", "Reconnecting for 42s"));
}

#[gpui_kit::test]
async fn a_failed_icons_tooltip_gives_the_reason(cx: &mut TestAppContext) {
    init(cx);
    set_state(cx, "kind-dev", State::Failed("connection refused"));
    let mut vcx = bar_window(cx, "kind-dev", None, Duration::from_secs(42));
    assert!(tooltip_reads(
        &mut vcx,
        "kind-dev",
        "Connection failed for 42s: connection refused"
    ));
}
