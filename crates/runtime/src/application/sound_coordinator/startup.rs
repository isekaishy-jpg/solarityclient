//! Immutable sound tables; SDL output, CVar publication and voice owners stay on main.

use solarity_asset::{
    AssetError, AssetStore, MovementSoundCatalog, ZoneSoundCatalog, ZoneSoundOverrideCatalog,
};
use solarity_media::SpatialSoundCatalog;

pub(in crate::application) struct PreparedSoundSources {
    pub(in crate::application) engine: Result<SpatialSoundCatalog, AssetError>,
    pub(in crate::application) world: Result<PreparedWorldSounds, AssetError>,
}

pub(in crate::application) struct PreparedWorldSounds {
    pub(super) movement_sounds: MovementSoundCatalog,
    pub(super) zones: ZoneSoundCatalog,
    pub(super) zone_overrides: ZoneSoundOverrideCatalog,
}

impl PreparedSoundSources {
    /// Retains failures until their original native-output/publication boundary.
    pub(in crate::application) fn load(store: &mut AssetStore) -> Self {
        let engine = SpatialSoundCatalog::load(store);
        let world = (|| {
            Ok(PreparedWorldSounds {
                movement_sounds: MovementSoundCatalog::load(store)?,
                zones: ZoneSoundCatalog::load(store)?,
                zone_overrides: ZoneSoundOverrideCatalog::load(store)?,
            })
        })();
        Self { engine, world }
    }
}
