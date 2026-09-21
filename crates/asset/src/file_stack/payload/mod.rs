//! Archive byte ownership retains admission across decoder and cache transfers.

mod budget;
mod bytes;
mod text;

pub use budget::AssetReadBudget;
pub use bytes::AssetBytes;

pub use text::AssetText;
