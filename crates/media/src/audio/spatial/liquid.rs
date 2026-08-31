//! Liquid-query sound resolution kept separate from terrain MCSE emitters.

use solarity_asset::{
    AssetError, AssetStore, LiquidTypeCatalog, LiquidTypeDefinition, SoundEntry, SoundEntryCatalog,
};
use thiserror::Error;

/// Build-12340 tables required to resolve a queried liquid's base sound.
pub struct LiquidSoundCatalog {
    liquid_types: LiquidTypeCatalog,
    sound_entries: SoundEntryCatalog,
}

impl LiquidSoundCatalog {
    /// Loads `LiquidType.dbc` and `SoundEntries.dbc` through archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when either exact stock table cannot be loaded and
    /// validated.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        Ok(Self {
            liquid_types: LiquidTypeCatalog::load(store)?,
            sound_entries: SoundEntryCatalog::load(store)?,
        })
    }

    /// Resolves `liquid query -> LiquidType -> SoundEntries` exactly.
    ///
    /// This path uses the base sound key in `LiquidType.dbc`. It does not pass
    /// through `SoundEntriesAdvanced.dbc`, MCSE, or the separately loaded but
    /// playback-unreferenced `SoundEmitters.dbc` table.
    ///
    /// # Errors
    ///
    /// Returns [`LiquidSoundError`] when either authored key is absent.
    pub fn resolve(
        &self,
        liquid_type_id: u32,
    ) -> Result<ResolvedLiquidSound<'_>, LiquidSoundError> {
        let liquid_type = self
            .liquid_types
            .entry(liquid_type_id)
            .ok_or(LiquidSoundError::MissingLiquidType { liquid_type_id })?;
        let sound_entry_id = liquid_type.sound_entry_id();
        let sound_entry = self.sound_entries.entry(sound_entry_id).ok_or(
            LiquidSoundError::MissingSoundEntry {
                liquid_type_id,
                sound_entry_id,
            },
        )?;
        Ok(ResolvedLiquidSound {
            liquid_type,
            sound_entry,
        })
    }
}

/// One queried liquid with its exact base sound row.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedLiquidSound<'catalog> {
    liquid_type: &'catalog LiquidTypeDefinition,
    sound_entry: &'catalog SoundEntry,
}

impl<'catalog> ResolvedLiquidSound<'catalog> {
    /// Returns the queried liquid definition and its stock flags.
    #[must_use]
    pub const fn liquid_type(self) -> &'catalog LiquidTypeDefinition {
        self.liquid_type
    }

    /// Returns the base sound row named directly by `LiquidType.dbc`.
    #[must_use]
    pub const fn sound_entry(self) -> &'catalog SoundEntry {
        self.sound_entry
    }
}

/// Failure to exactly resolve one liquid-query sound.
#[derive(Debug, Eq, Error, PartialEq)]
pub enum LiquidSoundError {
    /// The liquid query returned an absent `LiquidType.dbc` key.
    #[error("LiquidType.dbc does not contain queried liquid {liquid_type_id}")]
    MissingLiquidType {
        /// Exact liquid identifier returned by collision query.
        liquid_type_id: u32,
    },
    /// The liquid row names an absent base sound.
    #[error("liquid type {liquid_type_id} references absent SoundEntries.dbc row {sound_entry_id}")]
    MissingSoundEntry {
        /// Exact queried liquid identifier.
        liquid_type_id: u32,
        /// Exact base sound identifier stored by the liquid row.
        sound_entry_id: u32,
    },
}
