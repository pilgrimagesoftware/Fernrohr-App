//! The Resource panel: a connected window's list of every resource kind its
//! cluster's API discovery reports, CRDs included.
//!
//! Section 8.1 of the `cluster-picker-and-navigation` change replaces the fixed
//! Pods/Logs sidebar with this. It owns no connection of its own - it reaches
//! the window's context through [`ClusterRegistry`], so the list and the panels
//! it opens all share one `ClusterSession` and switching between them never
//! reconnects.

use crate::cluster::connection::{ClusterConnection, ConnectionState};
use crate::cluster::discovery::{DiscoveredKind, discover_kinds};
use crate::cluster::session::ClusterRegistry;
use crate::nav::NavTarget;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::sidebar::{Sidebar, SidebarMenuItem};
use gpui_kit::*;

/// Emitted when the user picks a row, so the window can open that kind's
/// panel. Which panel that is stays [`NavTarget`]'s decision - the Resource
/// panel only reports what was picked.
#[derive(Clone)]
pub enum ResourceEvent {
    /// A kind the user selected, wrapped as the target the window should show.
    Open(NavTarget),
}

/// How far the list has got. Discovery needs a client, so the panel spends its
/// first moments in `WaitingForConnection` - between the user picking a context
/// and the probe landing - and only then asks what kinds exist.
enum ResourceState {
    WaitingForConnection,
    Loading,
    Loaded(Vec<DiscoveredKind>),
    Failed(String),
}

pub struct ResourcePanel {
    context_name: String,
    state: ResourceState,
    /// Whether a discovery request is in flight. The request itself is
    /// detached - nothing needs to cancel it, the panel is its only reader -
    /// so this flag is what stops a connection that flaps from racing two
    /// results into `state`.
    loading: bool,
    /// The kind the window's active panel is showing, so its row is marked
    /// active. Set by the window whenever it switches panels.
    selected: Option<NavTarget>,
}

impl ResourcePanel {
    pub fn new(context_name: String, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &context_name);
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.sync(&connection, cx)
        })
        .detach();

        let mut this = Self {
            context_name,
            state: ResourceState::WaitingForConnection,
            loading: false,
            selected: None,
        };
        this.sync(&connection, cx);
        this
    }

    /// Marks `target`'s row active, or clears the mark when given `None` (the
    /// window has no panel open for this context any more).
    pub fn set_selected(&mut self, target: Option<NavTarget>, cx: &mut Context<Self>) {
        if self.selected == target {
            return;
        }
        self.selected = target;
        cx.notify();
    }

    /// The kinds currently listed. Read by the tests, which assert the list
    /// rather than the rendered rows - the rows themselves are only reachable
    /// by simulating a click.
    #[cfg(test)]
    pub fn kinds(&self) -> Option<&[DiscoveredKind]> {
        match &self.state {
            ResourceState::Loaded(kinds) => Some(kinds),
            _ => None,
        }
    }

    /// Starts discovery as soon as the window's context has a client. A no-op
    /// while the state is already settled or a request is in flight.
    fn sync(&mut self, connection: &Entity<ClusterConnection>, cx: &mut Context<Self>) {
        if self.loading || matches!(self.state, ResourceState::Loaded(_)) {
            return;
        }
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        self.load(client.clone(), cx);
    }

    fn load(&mut self, client: kube::Client, cx: &mut Context<Self>) {
        self.loading = true;
        self.state = ResourceState::Loading;
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            let _ = tx.send(discover_kinds(client).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.loading = false;
                    this.state = match result {
                        Ok(kinds) => ResourceState::Loaded(kinds),
                        Err(error) => ResourceState::Failed(error.to_string()),
                    };
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// The sidebar's rows while there is nothing to list yet - a plain
    /// non-interactive row per state, so the panel's width and placement stay
    /// the same whether or not discovery has landed.
    fn status_row(label: String) -> SidebarMenuItem {
        SidebarMenuItem::new(label).disable(true)
    }

    /// The row for each discovered kind: its label, the target selecting it
    /// opens, and whether that target is the one currently showing. Splitting
    /// this out of `render_kinds` is what makes the list assertable - the
    /// rendered rows are otherwise only reachable by simulating a click.
    fn rows(&self, kinds: &[DiscoveredKind]) -> Vec<(String, NavTarget, bool)> {
        kinds
            .iter()
            .map(|kind| {
                let target = NavTarget::Kind(kind.clone());
                let active = self.selected.as_ref() == Some(&target);
                (target.label(), target, active)
            })
            .collect()
    }

    /// Asks the window to show `target`. The single request path behind both
    /// ways of opening a row: the double-click (9.1) and the context menu's
    /// "Open" (9.2). They differ only in how they get here, which is what
    /// makes them equivalent.
    fn request_open(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        cx.emit(ResourceEvent::Open(target));
        cx.notify();
    }

    fn render_kinds(&self, kinds: &[DiscoveredKind], cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let items: Vec<SidebarMenuItem> = self
            .rows(kinds)
            .into_iter()
            .map(|(label, target, active)| {
                let dbl = this.clone();
                let opened = target.clone();
                let menu_target = target.clone();
                let menu_panel = this.clone();
                SidebarMenuItem::new(label)
                    .icon(target.icon())
                    .active(active)
                    // A single click only arms the row; the second click of a
                    // double-click opens the panel (9.1).
                    .on_click(move |event, _window, cx| {
                        if event.click_count() < 2 {
                            return;
                        }
                        let _ = dbl.update(cx, |this, cx| this.request_open(opened.clone(), cx));
                    })
                    .context_menu(move |menu, _window, _cx| {
                        let target = menu_target.clone();
                        let panel = menu_panel.clone();
                        menu.item(PopupMenuItem::new("Open").on_click(
                            move |_event, _window, cx| {
                                let _ = panel
                                    .update(cx, |this, cx| this.request_open(target.clone(), cx));
                            },
                        ))
                    })
            })
            .collect();
        Sidebar::new("resources")
            .collapsible(false)
            .header(self.header(cx))
            .children(items)
            .into_any_element()
    }

    /// The sidebar's header: which cluster's resources these rows are. The
    /// seed of the spec's cluster dropdown - with one connection there is
    /// nothing to choose, so it is a label rather than a control.
    fn header(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.sidebar_foreground)
                    .child("Resources"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(self.context_name.clone()),
            )
    }
}

impl EventEmitter<ResourceEvent> for ResourcePanel {}

impl Render for ResourcePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            ResourceState::Loaded(kinds) if kinds.is_empty() => Sidebar::new("resources")
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row(
                    "This cluster reported no resource kinds.".to_string(),
                ))
                .into_any_element(),
            ResourceState::Loaded(kinds) => self.render_kinds(kinds, cx),
            ResourceState::WaitingForConnection => Sidebar::new("resources")
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row("Connecting...".to_string()))
                .into_any_element(),
            ResourceState::Loading => Sidebar::new("resources")
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row(
                    "Discovering resource kinds...".to_string(),
                ))
                .into_any_element(),
            ResourceState::Failed(reason) => Sidebar::new("resources")
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row(format!(
                    "Could not discover resource kinds: {reason}"
                )))
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ResourcePanel, ResourceState};
    use crate::cluster::discovery::DiscoveredKind;
    use crate::nav::{NavTarget, has_concrete_panel};
    use gpui_kit::TestAppContext;
    use kube::core::GroupVersionKind;

    fn kind(group: &str, kind: &str) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{}s", kind.to_lowercase()),
            namespaced: true,
        }
    }

    /// Section 8.1: what the panel lists is exactly what discovery returned -
    /// one row per discovered kind, with the CRD kinds among them. Driving the
    /// loaded state directly is the seam: the discovery call itself is covered
    /// by `cluster::discovery`'s fixture tests.
    #[gpui_kit::test]
    async fn a_loaded_panel_lists_every_discovered_kind_including_crds(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_, cx| ResourcePanel::new("kind-dev".to_string(), cx));

        let discovered = vec![
            kind("", "Pod"),
            kind("", "Service"),
            kind("apps", "Deployment"),
            kind("ferns.example.com", "Fern"),
        ];
        window
            .update(cx, |panel, _window, cx| {
                panel.state = ResourceState::Loaded(discovered.clone());
                cx.notify();
            })
            .unwrap();

        window
            .update(cx, |panel, _window, _cx| {
                let listed = panel.kinds().expect("the panel has loaded kinds");
                assert_eq!(listed.len(), discovered.len(), "one row per kind");

                let labels: Vec<String> = listed.iter().map(DiscoveredKind::label).collect();
                assert!(labels.contains(&"Pod".to_string()));
                assert!(labels.contains(&"Deployment · apps".to_string()));
                assert!(
                    labels.contains(&"Fern · ferns.example.com".to_string()),
                    "the CRD gets a row like any other kind: {labels:?}"
                );
            })
            .unwrap();
    }

    /// Section 8.2: every listed kind is openable - the built-in Pod maps to the
    /// Pods panel, and a CRD maps to a placeholder rather than to nothing. This
    /// is the same `rows` mapping the sidebar's click handlers are built from.
    #[gpui_kit::test]
    async fn every_row_opens_a_panel_and_the_crd_gets_a_placeholder(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_, cx| ResourcePanel::new("kind-dev".to_string(), cx));

        let fern = kind("ferns.example.com", "Fern");
        let pod = DiscoveredKind::pods();
        window
            .update(cx, |panel, _window, cx| {
                panel.state = ResourceState::Loaded(vec![fern.clone(), pod.clone()]);
                cx.notify();
            })
            .unwrap();

        window
            .update(cx, |panel, _window, _cx| {
                let rows = panel.rows(&[fern.clone(), pod.clone()]);
                assert_eq!(rows.len(), 2, "one row per kind");

                for (label, target, active) in &rows {
                    assert!(!label.is_empty(), "every row names the kind it opens");
                    assert!(
                        !active,
                        "nothing is selected before the window opens a panel"
                    );

                    let opened = match target {
                        NavTarget::Kind(kind) => kind,
                        NavTarget::Logs => panic!("a discovered kind, not Logs"),
                    };
                    // Whether or not this build has a concrete panel, the row
                    // resolves to a target `build_layout` can render.
                    let _ = has_concrete_panel(opened);
                }

                let (_, fern_target, _) = &rows[0];
                assert!(!has_concrete_panel(match fern_target {
                    NavTarget::Kind(kind) => kind,
                    NavTarget::Logs => unreachable!(),
                }));
                let (_, pod_target, _) = &rows[1];
                assert!(has_concrete_panel(match pod_target {
                    NavTarget::Kind(kind) => kind,
                    NavTarget::Logs => unreachable!(),
                }));
            })
            .unwrap();
    }

    /// The window marks the open panel's row, so the list shows where the user
    /// is. Only the selected kind is marked - the rest stay unselected.
    #[gpui_kit::test]
    async fn the_open_panels_row_is_the_only_one_marked(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_, cx| ResourcePanel::new("kind-dev".to_string(), cx));

        let kinds = vec![
            kind("", "Service"),
            DiscoveredKind::pods(),
            kind("ferns.example.com", "Fern"),
        ];
        window
            .update(cx, |panel, _window, cx| {
                panel.state = ResourceState::Loaded(kinds.clone());
                panel.set_selected(Some(NavTarget::pods()), cx);
            })
            .unwrap();

        window
            .update(cx, |panel, _window, _cx| {
                let rows = panel.rows(&kinds);
                let marked: Vec<&str> = rows
                    .iter()
                    .filter(|(_, _, active)| *active)
                    .map(|(label, _, _)| label.as_str())
                    .collect();
                assert_eq!(marked, vec!["Pod"]);
            })
            .unwrap();
    }

    /// Section 9.2: the row's context-menu "Open" and its double-click are
    /// required to be equivalent. They are equivalent here by construction -
    /// both closures capture the same `target` and call the same
    /// `request_open` - so what this pins is that the shared path emits exactly
    /// one request for the row's own kind, which is what a second selector
    /// would otherwise turn into two panels.
    #[gpui_kit::test]
    async fn opening_a_row_emits_one_request_for_that_rows_kind(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_, cx| ResourcePanel::new("kind-dev".to_string(), cx));

        let fern = kind("ferns.example.com", "Fern");
        let opened: Vec<NavTarget> = Vec::new();
        let collected = std::rc::Rc::new(std::cell::RefCell::new(opened));

        window
            .update(cx, |panel, window, cx| {
                panel.state = ResourceState::Loaded(vec![fern.clone()]);
                let collected = collected.clone();
                let entity = cx.entity();
                cx.subscribe_in(
                    &entity,
                    window,
                    move |_panel, _entity, event, _window, _cx| {
                        let super::ResourceEvent::Open(target) = event;
                        collected.borrow_mut().push(target.clone());
                    },
                )
                .detach();
                cx.notify();
            })
            .unwrap();

        // Whatever selector fired - double-click or "Open" - it lands here.
        window
            .update(cx, |panel, _window, cx| {
                panel.request_open(NavTarget::Kind(fern.clone()), cx);
            })
            .unwrap();
        cx.run_until_parked();

        let opened = collected.borrow();
        assert_eq!(opened.len(), 1, "one request, so one panel");
        assert_eq!(opened[0], NavTarget::Kind(fern));
    }

    /// A cluster that reported nothing still renders the sidebar rather than
    /// collapsing the window's left edge away.
    #[gpui_kit::test]
    async fn an_empty_discovery_still_leaves_a_panel(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_, cx| ResourcePanel::new("kind-dev".to_string(), cx));

        window
            .update(cx, |panel, _window, cx| {
                panel.state = ResourceState::Loaded(Vec::new());
                cx.notify();
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.kinds(), Some(&[][..]));
            })
            .unwrap();
    }

    /// A discovery failure is reported rather than swallowed, and leaves the
    /// panel with no list rather than a stale one.
    #[gpui_kit::test]
    async fn a_failed_discovery_is_reported_and_lists_nothing(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_, cx| ResourcePanel::new("kind-dev".to_string(), cx));

        window
            .update(cx, |panel, _window, cx| {
                panel.state = ResourceState::Failed("connection refused".to_string());
                cx.notify();
            })
            .unwrap();

        window
            .update(cx, |panel, _window, _cx| {
                assert!(panel.kinds().is_none(), "no list on failure");
            })
            .unwrap();
    }
}
