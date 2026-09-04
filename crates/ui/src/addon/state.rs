//! Shared script-visible AddOn load progress.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::{AddonCatalog, AddonDefinition};

#[derive(Clone, Debug)]
struct AddonLoadEntry {
    definition: AddonDefinition,
    loaded: bool,
    finished: bool,
    enabled_for_all: bool,
    character_overrides: BTreeMap<String, bool>,
    saved_enabled_for_all: bool,
    saved_character_overrides: BTreeMap<String, bool>,
}

/// Ordered AddOn load flags shared by the loader and FrameXML Lua globals.
///
/// Stock distinguishes a module that has entered loading from one whose TOC
/// has finished executing, and `IsAddOnLoaded` exposes both flags.
#[derive(Clone, Debug, Default)]
pub struct UiAddonLoadState {
    entries: Rc<RefCell<Vec<AddonLoadEntry>>>,
}

impl UiAddonLoadState {
    /// Creates unloaded entries in the catalog's deterministic enumeration order.
    #[must_use]
    pub fn from_catalog(catalog: &AddonCatalog) -> Self {
        Self {
            entries: Rc::new(RefCell::new(
                catalog
                    .addons()
                    .iter()
                    .map(|addon| AddonLoadEntry {
                        definition: addon.clone(),
                        loaded: false,
                        finished: false,
                        enabled_for_all: addon.is_enabled_by_default(),
                        character_overrides: BTreeMap::new(),
                        saved_enabled_for_all: addon.is_enabled_by_default(),
                        saved_character_overrides: BTreeMap::new(),
                    })
                    .collect(),
            )),
        }
    }

    /// Returns the one-based number of catalog entries visible to Lua.
    #[must_use]
    pub fn addon_count(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Returns load and completion flags for a one-based catalog index.
    #[must_use]
    pub fn status_by_index(&self, index: usize) -> Option<(bool, bool)> {
        self.entries
            .borrow()
            .get(index.checked_sub(1)?)
            .map(|entry| (entry.loaded, entry.finished))
    }

    /// Returns load and completion flags for a case-insensitive folder name.
    #[must_use]
    pub fn status_by_name(&self, name: &str) -> Option<(bool, bool)> {
        self.entries
            .borrow()
            .iter()
            .find(|entry| entry.definition.name().eq_ignore_ascii_case(name))
            .map(|entry| (entry.loaded, entry.finished))
    }

    /// Returns a cloned catalog definition for one script-visible index.
    #[must_use]
    pub(crate) fn definition_by_index(&self, index: usize) -> Option<AddonDefinition> {
        self.entries
            .borrow()
            .get(index.checked_sub(1)?)
            .map(|entry| entry.definition.clone())
    }

    /// Returns one definition selected by its case-insensitive folder name.
    #[must_use]
    pub(crate) fn definition_by_name(&self, name: &str) -> Option<AddonDefinition> {
        self.entries
            .borrow()
            .iter()
            .find(|entry| entry.definition.name().eq_ignore_ascii_case(name))
            .map(|entry| entry.definition.clone())
    }

    /// Resolves a case-insensitive folder identity to its one-based UI index.
    #[must_use]
    pub(crate) fn index_by_name(&self, name: &str) -> Option<usize> {
        self.entries
            .borrow()
            .iter()
            .position(|entry| entry.definition.name().eq_ignore_ascii_case(name))
            .map(|index| index + 1)
    }

    /// Returns stock's tri-state AddOn enablement value for one selection.
    #[must_use]
    pub(crate) fn enable_state(&self, character: Option<&str>, index: usize) -> Option<u8> {
        let entries = self.entries.borrow();
        let entry = entries.get(index.checked_sub(1)?)?;
        let fully_enabled =
            entry.enabled_for_all && entry.character_overrides.values().all(|enabled| *enabled);
        if character.is_none() {
            let partly_enabled =
                entry.enabled_for_all || entry.character_overrides.values().any(|enabled| *enabled);
            return Some(if fully_enabled {
                2
            } else if partly_enabled {
                1
            } else {
                0
            });
        }
        let enabled = character
            .and_then(|name| entry.character_overrides.get(&name.to_ascii_lowercase()))
            .copied()
            .unwrap_or(entry.enabled_for_all);
        Some(if !enabled {
            0
        } else if fully_enabled {
            2
        } else {
            1
        })
    }

    /// Stages one AddOn's enablement for all characters or one named character.
    pub(crate) fn set_enabled(&self, character: Option<&str>, index: usize, enabled: bool) -> bool {
        let mut entries = self.entries.borrow_mut();
        let Some(entry) = index
            .checked_sub(1)
            .and_then(|index| entries.get_mut(index))
        else {
            return false;
        };
        match character {
            Some(name) => {
                entry
                    .character_overrides
                    .insert(name.to_ascii_lowercase(), enabled);
            }
            None => {
                entry.enabled_for_all = enabled;
                entry.character_overrides.clear();
            }
        }
        true
    }

    /// Commits staged AddOn selections as the reset baseline.
    pub(crate) fn save_enablement(&self) {
        for entry in self.entries.borrow_mut().iter_mut() {
            entry.saved_enabled_for_all = entry.enabled_for_all;
            entry.saved_character_overrides = entry.character_overrides.clone();
        }
    }

    /// Restores the last saved AddOn selections after a cancelled dialog.
    pub(crate) fn reset_enablement(&self) {
        for entry in self.entries.borrow_mut().iter_mut() {
            entry.enabled_for_all = entry.saved_enabled_for_all;
            entry.character_overrides = entry.saved_character_overrides.clone();
        }
    }

    /// Updates loader progress for an existing catalog entry.
    ///
    /// Returns `false` when no AddOn with that folder identity was discovered.
    pub fn set_status(&self, name: &str, loaded: bool, finished: bool) -> bool {
        let mut entries = self.entries.borrow_mut();
        let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.definition.name().eq_ignore_ascii_case(name))
        else {
            return false;
        };
        entry.loaded = loaded;
        entry.finished = finished;
        true
    }
}
