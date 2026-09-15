//! Immutable archive-stack identity and resource keys issued by the asset owner.

use crate::{AssetError, AssetPath};
use std::sync::atomic::{AtomicU64, Ordering};

/// One validated immutable mount plan, including locale and patch selection.
/// Cloned catalogs share it; rediscovery establishes a new generation even at
/// identical filesystem paths. Mounted handle identity remains separate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetNamespaceId(u64);

impl AssetNamespaceId {
    /// Issues a never-recycled identity without probing any individual asset.
    pub(crate) fn issue() -> Result<Self, AssetError> {
        next_identity().map(Self)
    }
}

/// Allocates namespace and handle identities without wrapping onto live owners.
pub(super) fn next_identity() -> Result<u64, AssetError> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    })
    .map_err(|_| AssetError::IdentityExhausted)
}

/// A source path qualified by the exact immutable archive selection.
/// Resource kind and decode options belong to each domain's typed request key.
/// Cloning the path shares its canonical string; lookups do not duplicate bytes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AssetResourceKey {
    namespace: AssetNamespaceId,
    path: AssetPath,
}

impl AssetResourceKey {
    /// Takes a normalized path under an asset-issued namespace.
    #[must_use]
    pub const fn new(namespace: AssetNamespaceId, path: AssetPath) -> Self {
        Self { namespace, path }
    }
    /// Returns the immutable archive selection carried by this key.
    #[must_use]
    pub const fn namespace(&self) -> AssetNamespaceId {
        self.namespace
    }
    /// Returns the canonical source path without copying its shared bytes.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }
}
