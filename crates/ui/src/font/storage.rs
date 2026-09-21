//! Font storage uses the shared asset allocation owners.
pub(super) use solarity_asset::AssetStorageMap as CacheMap;
pub(crate) use solarity_asset::AssetStorageVec as FontBuffer;

#[cfg(test)]
#[path = "../../tests/font/storage.rs"]
mod tests;
