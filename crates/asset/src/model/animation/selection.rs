//! CM2Model animation fallback, independent of gameplay's tiered behavior lookup.

use crate::AnimationDataCatalog;

use super::M2AnimationSet;

/// Playback operation accumulated while following AnimationData fallback rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2ModelAnimationMode {
    /// Play the selected sequence from its beginning.
    Forward,
    /// Play backward from the end of the selected cycle count.
    Reverse,
    /// Keep the first pose without advancing the sequence timer.
    HoldStart,
    /// Keep the terminal pose without advancing the sequence timer.
    HoldEnd,
}

/// Model-local sequence identity and playback mode resolved by `CM2Model`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2ModelAnimation {
    animation_id: u16,
    mode: M2ModelAnimationMode,
}

impl M2ModelAnimation {
    /// Returns the animation ID in the selected model's sequence table.
    #[must_use]
    pub const fn animation_id(self) -> u16 {
        self.animation_id
    }

    /// Returns the operation accumulated along the fallback chain.
    #[must_use]
    pub const fn mode(self) -> M2ModelAnimationMode {
        self.mode
    }
}

impl M2AnimationSet {
    /// Tests authored sequence presence as build-12340 `0x00825E00` does.
    ///
    /// External payload availability and AnimationData fallbacks are separate.
    #[must_use]
    pub fn has_model_animation(&self, animation_id: u16) -> bool {
        self.lookup_sequence(animation_id).is_some()
    }

    /// Returns the first authored variation's duration after Model fallback.
    ///
    /// GameObject seeks use `0x0082CED0` with variation ordinal zero. This is
    /// the lookup head, independently of its variation ID, payload alias,
    /// external-data availability, and the variation selected for playback.
    #[must_use]
    pub fn model_animation_duration_ms(
        &self,
        catalog: &AnimationDataCatalog,
        requested_animation: u32,
    ) -> Option<u32> {
        let resolved = self.resolve_model_animation(catalog, requested_animation)?;
        let index = self.lookup_sequence(resolved.animation_id())?;
        Some(self.sequences.get(index)?.duration_ms())
    }

    /// Selects the variation requested by the Model Lua bridge (`0x00832AB0`).
    ///
    /// Its unspecified-variation argument always runs `0x00826E60`, even if
    /// variation zero exists. Selection precedes external-payload availability;
    /// the caller must defer playback if the selected payload is unavailable.
    #[must_use]
    pub fn select_model_sequence(&self, animation_id: u16, weighted_roll: u16) -> Option<usize> {
        let first = self.lookup_sequence(animation_id)?;
        // 0x008260C0 interprets the requested variation as a chain ordinal.
        // Ordinal zero is the lookup head, regardless of its metadata ID.
        let base = first;
        let mut roll = u32::from(weighted_roll);
        let mut selected = None;
        self.visit_variations(base, animation_id, |index, sequence| {
            if selected.is_some() {
                return;
            }
            let weight = sequence.frequency;
            if roll < weight {
                selected = Some(index);
            } else {
                roll -= weight;
            }
        })?;
        Some(selected.unwrap_or(base))
    }

    /// Resolves a model request through build-12340 `0x00826350`.
    ///
    /// Direct sequence presence wins before consulting AnimationData. Missing
    /// requests follow its fallback IDs, accumulating reverse (`0x10`) and
    /// endpoint (`0x20`) flags. A broken chain uses Stand, then animation 147,
    /// then the first authored sequence, with ordinary forward playback.
    /// This lookup considers authored presence even when an external payload
    /// is not yet available; loading and starting that sequence are separate.
    /// Returns `None` for a model without sequence records.
    #[must_use]
    pub fn resolve_model_animation(
        &self,
        catalog: &AnimationDataCatalog,
        requested_animation: u32,
    ) -> Option<M2ModelAnimation> {
        let contains = |id| {
            u16::try_from(id)
                .ok()
                .is_some_and(|id| self.lookup_sequence(id).is_some())
        };
        let emergency = M2ModelAnimation {
            animation_id: if contains(0) {
                0
            } else if contains(147) {
                147
            } else {
                self.sequences.first()?.animation_id
            },
            mode: M2ModelAnimationMode::Forward,
        };
        let mut visited = [false; 506];
        let mut current = requested_animation;
        let mut direction = 1_i32;
        let mut endpoint = 0_i32;
        while !contains(current) {
            let Some(seen) = visited.get_mut(current as usize) else {
                return Some(emergency);
            };
            let Some(row) = catalog.definition(current) else {
                return Some(emergency);
            };
            if *seen || row.fallback_id() == current {
                return Some(emergency);
            }
            *seen = true;
            if row.flags() & 0x10 != 0 {
                endpoint += direction;
                direction = -direction;
            }
            if row.flags() & 0x20 != 0 {
                endpoint += direction;
                direction = 0;
            }
            current = row.fallback_id();
        }
        Some(M2ModelAnimation {
            // `contains` above proves that the selected ID fits the M2 field.
            animation_id: current as u16,
            mode: match direction {
                1 => M2ModelAnimationMode::Forward,
                -1 => M2ModelAnimationMode::Reverse,
                _ if endpoint > 0 => M2ModelAnimationMode::HoldEnd,
                _ => M2ModelAnimationMode::HoldStart,
            },
        })
    }
}
