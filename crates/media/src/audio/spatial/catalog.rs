//! Exact terrain-emitter joins without inferred playback behavior.

use solarity_asset::{
    AdvancedSoundEntry, AdvancedSoundEntryCatalog, AssetError, AssetStore, SoundEntry,
    SoundEntryCatalog, UiSoundLookupCatalog,
};
use thiserror::Error;

/// Build-12340 sound tables required to resolve a positioned terrain emitter.
pub struct SpatialSoundCatalog {
    advanced_entries: AdvancedSoundEntryCatalog,
    sound_entries: SoundEntryCatalog,
    ui_sound_lookups: UiSoundLookupCatalog,
}

impl SpatialSoundCatalog {
    /// Loads both stock sound tables through the mounted archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when either exact DBC cannot be loaded and
    /// validated.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        Ok(Self {
            advanced_entries: AdvancedSoundEntryCatalog::load(store)?,
            sound_entries: SoundEntryCatalog::load(store)?,
            ui_sound_lookups: UiSoundLookupCatalog::load(store)?,
        })
    }

    /// Resolves `MCSE -> SoundEntriesAdvanced -> SoundEntries` exactly.
    ///
    /// Callers pass [`solarity_asset::TerrainSoundEmitter::advanced_sound_entry_id`]
    /// without treating it as a base sound identifier. The result does not
    /// interpret advanced timing, cone, influence, or ducking fields.
    ///
    /// # Errors
    ///
    /// Returns [`SpatialSoundError`] when either authored key is absent. No
    /// neighboring advanced or base sound row is substituted.
    pub fn resolve(
        &self,
        advanced_entry_id: u32,
    ) -> Result<ResolvedSpatialSound<'_>, SpatialSoundError> {
        let advanced_entry = self
            .advanced_entries
            .entry(advanced_entry_id)
            .ok_or(SpatialSoundError::MissingAdvancedEntry { advanced_entry_id })?;
        let sound_entry_id = advanced_entry.sound_entry_id();
        let sound_entry = self.sound_entries.entry(sound_entry_id).ok_or(
            SpatialSoundError::MissingSoundEntry {
                advanced_entry_id,
                sound_entry_id,
            },
        )?;
        Ok(ResolvedSpatialSound {
            advanced_entry,
            sound_entry,
        })
    }

    /// Finds one base sound for ordinary non-advanced playback paths.
    pub(in crate::audio) fn sound_entry(&self, sound_entry_id: u32) -> Option<&SoundEntry> {
        self.sound_entries.entry(sound_entry_id)
    }

    /// Resolves the string arm of build 12340's `PlaySound` function.
    ///
    /// GlueXML prefixes historical UI lookup constants with `gs`, while the
    /// related `UISoundLookups.dbc` names omit that prefix. The executable's
    /// name map then permits an internal `SoundEntries` name as the second
    /// exact namespace.
    pub(in crate::audio) fn script_sound_entry(&self, name: &str) -> Option<&SoundEntry> {
        let ui_name = name
            .get(..2)
            .filter(|prefix| prefix.eq_ignore_ascii_case("gs"))
            .map_or(name, |_prefix| &name[2..]);
        self.ui_sound_lookups
            .entry_by_name(ui_name)
            .and_then(|lookup| self.sound_entries.entry(lookup.sound_entry_id()))
            .or_else(|| self.sound_entries.entry_by_internal_name(name))
    }

    /// Resolves named Glue music and ambience through `SoundEntries` only.
    pub(in crate::audio) fn sound_entry_by_internal_name(&self, name: &str) -> Option<&SoundEntry> {
        self.sound_entries.entry_by_internal_name(name)
    }
}

/// One terrain emitter with both of its exact build-12340 sound rows.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedSpatialSound<'catalog> {
    advanced_entry: &'catalog AdvancedSoundEntry,
    sound_entry: &'catalog SoundEntry,
}

impl<'catalog> ResolvedSpatialSound<'catalog> {
    /// Returns the exact advanced policy row named by MCSE.
    #[must_use]
    pub const fn advanced_entry(self) -> &'catalog AdvancedSoundEntry {
        self.advanced_entry
    }

    /// Returns the exact base sound row named by the advanced policy row.
    #[must_use]
    pub const fn sound_entry(self) -> &'catalog SoundEntry {
        self.sound_entry
    }
}

/// Failure to exactly join one positioned stock sound.
#[derive(Debug, Eq, Error, PartialEq)]
pub enum SpatialSoundError {
    /// The MCSE key is absent from `SoundEntriesAdvanced.dbc`.
    #[error("SoundEntriesAdvanced.dbc does not contain terrain sound {advanced_entry_id}")]
    MissingAdvancedEntry {
        /// Exact advanced identifier stored by MCSE.
        advanced_entry_id: u32,
    },
    /// The advanced row names an absent `SoundEntries.dbc` row.
    #[error(
        "terrain sound {advanced_entry_id} references absent SoundEntries.dbc row {sound_entry_id}"
    )]
    MissingSoundEntry {
        /// Exact advanced identifier stored by MCSE.
        advanced_entry_id: u32,
        /// Exact base identifier stored by the advanced row.
        sound_entry_id: u32,
    },
}
