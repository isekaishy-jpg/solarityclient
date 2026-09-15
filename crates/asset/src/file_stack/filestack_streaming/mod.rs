//! Mounted archive ownership, staged opening and exact-precedence reads.

mod mount;
mod store;

pub use mount::AssetMount;
pub use store::{AssetRead, AssetStore};
