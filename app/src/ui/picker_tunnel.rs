//! Section 3.1 of the `tunnel-management-ui` change: the per-row tunnel selector on
//! the cluster picker.
//!
//! Kept out of `ui/picker.rs` (already close to the file-size limit, per design.md
//! decision 6) even though [`selector`] is only ever called from there. Binding is set
//! from the context side per proposal.md: this renders "Direct" or the bound tunnel's
//! name on a row, with a dropdown of every configured tunnel plus "Direct", and writes
//! straight through [`TunnelStore::bind`]/[`TunnelStore::unbind`] - `tunnels.toml` is
//! the only state, so the caller just re-reads it after a pick rather than this module
//! caching anything.

use crate::tunnel::store::TunnelStore;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::*;
use std::rc::Rc;

/// One tunnel offered by [`selector`]: its stable id (what a binding names) and its
/// current display name (what the row and menu show).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelChoice {
    pub id: String,
    pub name: String,
}

/// Every tunnel `store` knows about, as [`selector`] choices - sorted by name (then id,
/// to break ties deterministically) so the dropdown reads the same on every render
/// regardless of `tunnels.toml`'s own (unordered `BTreeMap`-by-id) iteration order.
pub fn tunnel_choices(store: &TunnelStore) -> Vec<TunnelChoice> {
    let mut choices: Vec<TunnelChoice> = store
        .list()
        .into_iter()
        .map(|(id, tunnel)| TunnelChoice {
            id,
            name: tunnel.name,
        })
        .collect();
    choices.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    choices
}

/// The label a row shows for its current binding: the bound tunnel's name, or
/// "Direct" for an unbound context - the same wording `spec.md`'s "Binding visible on
/// the context" scenario asks for.
pub fn bound_label(choices: &[TunnelChoice], bound_id: Option<&str>) -> String {
    match bound_id {
        None => "Direct".to_string(),
        Some(id) => choices
            .iter()
            .find(|choice| choice.id == id)
            .map(|choice| choice.name.clone())
            .unwrap_or_else(|| "Direct".to_string()),
    }
}

/// A selected tunnel (or "Direct") callback, boxed once so [`selector`]'s dropdown
/// items can each hold a cheap clone of the same handler rather than a distinct
/// generic closure per item.
type OnPick = Rc<dyn Fn(Option<String>, &mut App)>;

/// Renders one row's tunnel selector: a compact button showing the current binding's
/// name (or "Direct"), opening a dropdown of "Direct" plus every tunnel in `choices`.
/// `on_pick` receives `Some(tunnel_id)` for a tunnel or `None` for "Direct" - the
/// caller (here, `ClusterPicker`) does the actual `TunnelStore::bind`/`unbind` write,
/// since it also owns the store's path and the cache to refresh afterward.
///
/// A click here never reaches the row's own `on_click` (which would otherwise start a
/// connection): `Button`'s own click handling calls `cx.stop_propagation()`, so this is
/// safe to embed inside a `Command` list row (see `command/state.rs`'s `render_item`).
pub fn selector(
    row_id: impl Into<ElementId>,
    choices: &[TunnelChoice],
    bound_id: Option<&str>,
    on_pick: impl Fn(Option<String>, &mut App) + 'static,
) -> impl IntoElement {
    let label = bound_label(choices, bound_id);
    let current_id = bound_id.map(str::to_string);
    let choices = choices.to_vec();
    let on_pick: OnPick = Rc::new(on_pick);

    Button::new(row_id)
        .label(label)
        .icon(IconName::ChevronDown)
        .xsmall()
        .ghost()
        .tab_stop(false)
        .tooltip("Tunnel")
        .dropdown_menu(move |menu, _window, _cx| {
            // Built per open rather than hoisted: `PopupMenuItem` is not `Clone`, and
            // this closure is `Fn` so it can run more than once - see
            // `panel/title.rs::namespace_picker` for the same shape.
            let mut menu = menu.item(
                PopupMenuItem::new("Direct")
                    .checked(current_id.is_none())
                    .on_click({
                        let on_pick = on_pick.clone();
                        move |_event, _window, cx| on_pick(None, cx)
                    }),
            );
            for choice in &choices {
                let id = choice.id.clone();
                let checked = current_id.as_deref() == Some(id.as_str());
                let on_pick = on_pick.clone();
                menu = menu.item(
                    PopupMenuItem::new(choice.name.clone())
                        .checked(checked)
                        .on_click(move |_event, _window, cx| on_pick(Some(id.clone()), cx)),
                );
            }
            menu
        })
}

#[cfg(test)]
mod tests {
    use super::{TunnelChoice, bound_label, tunnel_choices};
    use crate::config::tunnels::TunnelConfig;
    use crate::tunnel::store::TunnelStore;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fernrohr-picker-tunnel-test-{}-{n}.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn sample_tunnel(name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.to_string(),
            bastion_user: "ops".into(),
            bastion_host: "bastion.example.com".into(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            auth: crate::config::tunnels::TunnelAuth::default(),
        }
    }

    #[test]
    fn choices_are_sorted_by_name() {
        let store = TunnelStore::new(temp_path());
        store.create("b-id", sample_tunnel("Zebra"), None).unwrap();
        store.create("a-id", sample_tunnel("Apple"), None).unwrap();

        let choices = tunnel_choices(&store);

        assert_eq!(
            choices,
            vec![
                TunnelChoice {
                    id: "a-id".to_string(),
                    name: "Apple".to_string()
                },
                TunnelChoice {
                    id: "b-id".to_string(),
                    name: "Zebra".to_string()
                },
            ]
        );
    }

    #[test]
    fn unbound_reads_direct() {
        assert_eq!(bound_label(&[], None), "Direct");
    }

    #[test]
    fn bound_reads_the_tunnels_name() {
        let choices = vec![TunnelChoice {
            id: "qa-bastion".to_string(),
            name: "QA Bastion".to_string(),
        }];
        assert_eq!(bound_label(&choices, Some("qa-bastion")), "QA Bastion");
    }

    /// A binding pointing at a tunnel id that no longer exists (e.g. deleted from
    /// under the picker) reads as Direct rather than panicking or showing a raw id.
    #[test]
    fn a_binding_to_a_missing_tunnel_reads_direct() {
        assert_eq!(bound_label(&[], Some("deleted-tunnel")), "Direct");
    }
}
