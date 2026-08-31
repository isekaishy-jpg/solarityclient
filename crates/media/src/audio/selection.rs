//! Weighted selection over the authored `SoundEntries.dbc` file slots.

use solarity_asset::{SoundAsset, SoundEntry};

/// Borrowed weighted file slots for one sound entry.
///
/// The selector deliberately accepts an already bounded weight ticket. Random
/// number generation and conversion to that ticket remain with the runtime's
/// process-wide stock CRT stream, so this layer cannot accidentally introduce
/// a second RNG or change call ordering.
#[derive(Clone, Copy, Debug)]
pub struct SoundVariationSelector<'entry> {
    entry: &'entry SoundEntry,
    total_weight: u64,
}

impl<'entry> SoundVariationSelector<'entry> {
    /// Builds a selector when the entry has at least one positive-weight file.
    ///
    /// Zero-frequency slots are stock-authored disabled variations. They remain
    /// visible through [`SoundEntry::assets`] but do not occupy ticket space.
    #[must_use]
    pub fn new(entry: &'entry SoundEntry) -> Option<Self> {
        let total_weight = entry
            .assets()
            .iter()
            .map(|asset| u64::from(asset.frequency()))
            .sum();
        (total_weight != 0).then_some(Self {
            entry,
            total_weight,
        })
    }

    /// Returns the exclusive upper bound for a caller-supplied weight ticket.
    #[must_use]
    pub const fn total_weight(self) -> u64 {
        self.total_weight
    }

    /// Resolves one ticket in `0..total_weight` to its authored file slot.
    ///
    /// Returning `None` for an out-of-range ticket keeps range conversion at
    /// the RNG-owning call site explicit. No modulo or clamping fallback is
    /// performed here.
    #[must_use]
    pub fn select(self, ticket: u64) -> Option<&'entry SoundAsset> {
        if ticket >= self.total_weight {
            return None;
        }

        let mut remaining = ticket;
        for asset in self.entry.assets() {
            let weight = u64::from(asset.frequency());
            if remaining < weight {
                return Some(asset);
            }
            remaining -= weight;
        }
        None
    }
}
