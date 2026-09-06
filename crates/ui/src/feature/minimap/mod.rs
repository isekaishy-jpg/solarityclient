//! Minimap frame controls and overlays above renderer-owned minimap composition.

mod minimap_frame;
mod state;
mod tracking;

pub(crate) use minimap_frame::MinimapWidgetState;
pub use state::UiMinimapState;
pub(crate) use tracking::register_globals;
pub use tracking::{UiMinimapTrackingState, UiTrackingCategory, UiTrackingError, UiTrackingType};
