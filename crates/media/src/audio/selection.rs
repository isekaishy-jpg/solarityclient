//! Stateful stock selection over authored `SoundEntries.dbc` file slots.

use solarity_asset::{SoundAsset, SoundEntry};

/// Selection behavior passed to build 12340's sound-definition routine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SoundVariationMode {
    /// Selects the first available slot after the previous one.
    Sequential = 0,
    /// Selects by weight and clears live weights after each selection.
    RandomReset = 1,
    /// Selects by weight while retaining live availability weights.
    Random = 2,
}

/// Per-`SoundEntries` selection state shared by every playback of that row.
///
/// Build 12340 retains the previous slot and a mutable copy of the ten authored
/// frequencies on its loaded sound definition. This owned form can live beside
/// the immutable asset catalog without creating a self-referential engine.
#[derive(Clone, Debug)]
pub struct SoundVariationSelector {
    assets: Vec<SoundAsset>,
    original_weights: Vec<u32>,
    current_weights: Vec<u32>,
    last_index: Option<usize>,
}

impl SoundVariationSelector {
    /// Copies the row's nonempty slots and their exact authored frequencies.
    ///
    /// An entry with no file names has no selector. Zero-frequency named slots
    /// remain present because stock sequential mode can activate them.
    #[must_use]
    pub fn new(entry: &SoundEntry) -> Option<Self> {
        if entry.assets().is_empty() {
            return None;
        }
        let assets = entry.assets().to_vec();
        let original_weights = assets.iter().map(SoundAsset::frequency).collect::<Vec<_>>();
        Some(Self {
            current_weights: original_weights.clone(),
            original_weights,
            assets,
            last_index: None,
        })
    }

    /// Selects one slot using stock last-slot exclusion and range conversion.
    ///
    /// Random modes consume one Blizzard PRNG word only after a nonzero bound
    /// has been prepared. Sequential mode always uses ticket zero and does not
    /// advance the supplied generator. If the previous slot is the only
    /// weighted choice, stock clears that exclusion on its third attempt.
    #[must_use]
    pub fn select(
        &mut self,
        mode: SoundVariationMode,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Option<&SoundAsset> {
        let total_weight = self.prepare_weight(mode)?;
        let mut ticket = match mode {
            SoundVariationMode::Sequential => 0,
            SoundVariationMode::RandomReset | SoundVariationMode::Random => {
                multiply_high_range(next_random_word(), total_weight)
            }
        };
        let selected_index =
            self.current_weights
                .iter()
                .enumerate()
                .find_map(|(index, weight)| {
                    if Some(index) == self.last_index {
                        return None;
                    }
                    if ticket < *weight {
                        Some(index)
                    } else {
                        ticket -= *weight;
                        None
                    }
                })?;

        self.last_index = Some(selected_index);
        if mode == SoundVariationMode::RandomReset {
            self.current_weights.fill(0);
        }
        self.assets.get(selected_index)
    }

    /// Returns the last selected nonempty-slot index in catalog order.
    #[must_use]
    pub const fn last_index(&self) -> Option<usize> {
        self.last_index
    }

    /// Prepares the exact selection bound, retrying stock weight resets.
    fn prepare_weight(&mut self, mode: SoundVariationMode) -> Option<u32> {
        for attempt in 0..3 {
            if attempt == 2 {
                self.last_index = None;
            }
            let total_weight = self
                .current_weights
                .iter()
                .enumerate()
                .filter(|(index, _weight)| Some(*index) != self.last_index)
                .fold(0_u32, |total, (_index, weight)| total.wrapping_add(*weight));
            if total_weight != 0 {
                return Some(total_weight);
            }

            match mode {
                SoundVariationMode::Sequential => self.current_weights.fill(1),
                SoundVariationMode::RandomReset | SoundVariationMode::Random => {
                    self.current_weights.clone_from(&self.original_weights);
                }
            }
        }
        None
    }
}

/// Maps one 32-bit random word into a 64-bit weight bound with multiply-high.
fn multiply_high_range(random_word: u32, total_weight: u32) -> u32 {
    ((u64::from(random_word) * u64::from(total_weight)) >> u32::BITS) as u32
}
