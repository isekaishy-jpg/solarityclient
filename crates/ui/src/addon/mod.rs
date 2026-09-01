//! AddOn discovery, metadata, dependency order, saved state, and enablement.
//!
//! The stock `AddOns.cpp` boundary makes AddOn loading distinct from Lua
//! execution and XML construction. Archive/file-stack resolution is requested
//! through the asset facade so load order stays consistent with stock.

mod add_ons;
mod catalog;
mod error;
mod saved_variable;
mod state;
mod toc;

pub use add_ons::{AddonCompatibility, AddonDefinition};
pub use catalog::{AddonCatalog, STANDARD_ADDON_CRC, STOCK_INTERFACE_VERSION};
pub use error::AddonCatalogError;
pub use saved_variable::UiSavedVariableState;
pub use state::UiAddonLoadState;
