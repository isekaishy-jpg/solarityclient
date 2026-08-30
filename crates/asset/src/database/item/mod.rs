//! Typed build-12340 item display tables used by character equipment rendering.

mod definition;
mod display;

pub use definition::{InventoryType, ItemDefinition, ItemDefinitionCatalog};
pub use display::{ItemDisplayCatalog, ItemDisplayInfo};
