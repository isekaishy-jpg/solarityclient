//! Shared script-visible AddOn load progress.

use std::cell::RefCell;
use std::rc::Rc;

use super::{AddonCatalog, AddonDefinition};

#[derive(Clone, Debug)]
struct AddonLoadEntry {
    definition: AddonDefinition,
    loaded: bool,
    finished: bool,
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
