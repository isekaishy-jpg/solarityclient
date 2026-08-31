//! Shared ownership of encoded sound payloads selected by the archive stack.

use std::collections::HashMap;
use std::sync::Arc;

use solarity_asset::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};

/// One archive-selected WAV or MP3 payload retained for decoder lifetimes.
#[derive(Debug)]
pub struct EncodedSound {
    path: AssetPath,
    source: ArchiveDescriptor,
    bytes: Vec<u8>,
}

impl EncodedSound {
    /// Returns the single normalized cache and archive identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the exact archive chosen by ordinary stock precedence.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns encoded bytes without a decoder-specific intermediate copy.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Process-local cache of immutable encoded sound payloads.
///
/// Decoder voices retain [`Arc`] owners because SDL may consume the same sound
/// concurrently. The cache applies no guessed byte budget or quality tier;
/// callers explicitly collect entries after all external owners retire.
#[derive(Default)]
pub struct SoundCache {
    sounds: HashMap<AssetPath, Arc<EncodedSound>>,
}

impl SoundCache {
    /// Creates an empty cache without allocating its path table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of distinct encoded archive paths retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    /// Reports whether no encoded payloads are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    /// Returns a shared payload, reading its selected MPQ entry only once.
    ///
    /// The path is the only identity. Replacement sound packs therefore replace
    /// stock bytes through archive precedence instead of creating parallel
    /// cache entries or format-specific fallback searches.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the exact path cannot be read from the
    /// mounted stock stack. Failed reads are not retained.
    pub fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<Arc<EncodedSound>, AssetError> {
        if let Some(sound) = self.sounds.get(path) {
            return Ok(Arc::clone(sound));
        }

        let read = store.read(path)?;
        let source = read.source().clone();
        let sound = Arc::new(EncodedSound {
            path: path.clone(),
            source,
            bytes: read.into_bytes(),
        });
        self.sounds.insert(path.clone(), Arc::clone(&sound));
        Ok(sound)
    }

    /// Releases entries owned only by the cache and returns the removal count.
    pub fn collect_unused(&mut self) -> usize {
        let before = self.sounds.len();
        self.sounds
            .retain(|_path, sound| Arc::strong_count(sound) > 1);
        before - self.sounds.len()
    }
}
