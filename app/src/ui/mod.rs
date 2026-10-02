pub mod about_window;
pub mod accent;
pub mod context_bar;
pub mod detail;
pub mod link;
pub mod menu;
pub mod nav;
pub mod panel;
pub mod picker;
pub mod picker_keys;
pub mod picker_tunnel;
pub mod placeholder;
pub mod settings;
// UNWIRED: the panels convert to the tokens in `visual-refresh-typography-
// spacing` 3.2, and the text-size preference sets `TextScale` in section 4.
#[allow(dead_code)]
pub mod space;
pub mod status_bar;
pub mod style;
pub mod theme;
pub mod tunnels;
pub mod typography;
pub mod viewer;

pub use panel::resource as resource_panel;
pub use panel::title as panel_title;
