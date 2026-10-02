pub mod about_window;
pub mod accent;
pub mod context_bar;
pub mod detail;
// UNWIRED: the panels, kind list, cards and links draw these in
// `resource-kind-icons` section 3; until then only the tests use them.
#[allow(dead_code)]
pub mod icon;
pub mod link;
pub mod menu;
pub mod nav;
pub mod panel;
pub mod picker;
pub mod picker_keys;
pub mod picker_tunnel;
pub mod placeholder;
pub mod settings;
pub mod space;
pub mod status_bar;
pub mod style;
pub mod text_size;
pub mod theme;
pub mod tunnels;
pub mod typography;
pub mod viewer;

pub use panel::resource as resource_panel;
pub use panel::title as panel_title;
