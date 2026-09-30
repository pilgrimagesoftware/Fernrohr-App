//! The Logs dock panel: streaming a pod container's logs and displaying
//! them. Split by concern: [`view`] is the GPUI-free view-model
//! ([`view::LogsView`], its events and follow state), [`stream`] drives a
//! `kube` log stream onto that view-model, [`panel`] is the dock panel's
//! state and lifecycle, [`render`] draws it, and [`title`] is its dock
//! identity (saved-layout dump, title/tab/toolbar/zoom).

mod panel;
mod render;
mod stream;
mod title;
mod view;

use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use std::rc::Rc;

use stream::*;
use view::*;

pub use panel::{LogsPanel, register_restore};
