//! Stock listener-relative category ducking for advanced sound instances.

use super::sound_interface2_advanced_kit_properties::AdvancedSoundProperties;
use super::types::SoundCategory;

/// Stable identity of one live advanced sound instance.
///
/// Stock uses the live object's address for exclusion and attachment identity.
/// The 64-bit client architecture supplies an explicit stable value instead.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AdvancedSoundInstanceId(u64);

impl AdvancedSoundInstanceId {
    /// Wraps a caller-owned identity that remains unique while the sound lives.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the caller-owned identity.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Global advanced-sound ducking list and category gain aggregator.
///
/// Entries stay resident after leaving their influence volume until all three
/// category gains recover to `1.0`, matching the stock explicit list.
#[derive(Debug, Default)]
pub struct AdvancedSoundDucking {
    influences: Vec<DuckInfluence>,
}

impl AdvancedSoundDucking {
    /// Creates an empty process-wide ducking list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            influences: Vec::new(),
        }
    }

    /// Attaches one advanced instance if its row authors stock ducking policy.
    ///
    /// Stock allocates an influence only when at least one corrected category
    /// gain lies strictly between zero and one. An already attached instance
    /// is not duplicated. A detached entry still recovering has no instance
    /// identity and therefore does not block later re-entry.
    pub fn register(
        &mut self,
        source: AdvancedSoundInstanceId,
        properties: AdvancedSoundProperties,
    ) {
        let targets = properties.duck_gains();
        if !targets.iter().any(|gain| 0.0 < *gain && *gain < 1.0)
            || self
                .influences
                .iter()
                .any(|influence| influence.source == Some(source))
        {
            return;
        }

        self.influences.push(DuckInfluence {
            source: Some(source),
            targets,
            current: [1.0; 3],
            influence_radii: properties.influence_radii(),
            transition_milliseconds: properties.duck_transition_milliseconds(),
            latched: false,
        });
    }

    /// Detaches every influence owned by a destroyed advanced instance.
    ///
    /// Recovery begins on the next [`Self::update`] call; the entry is not
    /// removed early and its current gain continues to affect other sounds.
    pub fn remove_source(&mut self, source: AdvancedSoundInstanceId) {
        for influence in &mut self.influences {
            if influence.source == Some(source) {
                influence.source = None;
            }
        }
    }

    /// Advances influence hysteresis and category transition gains.
    ///
    /// `source_position` must return the current emitter position for each live
    /// attached instance. Returning `None` detaches a source before this tick's
    /// recovery, as destruction does in the stock service.
    pub fn update(
        &mut self,
        elapsed_milliseconds: i32,
        listener_position: [f32; 3],
        mut source_position: impl FnMut(AdvancedSoundInstanceId) -> Option<[f32; 3]>,
    ) {
        for influence in &mut self.influences {
            let emitter_position = influence.source.and_then(&mut source_position);
            if influence.source.is_some() && emitter_position.is_none() {
                influence.source = None;
            }
            influence.update(elapsed_milliseconds, listener_position, emitter_position);
        }
        self.influences
            .retain(|influence| influence.source.is_some() || influence.current != [1.0; 3]);
    }

    /// Returns the most suppressive current gain for one stock category.
    ///
    /// The requesting advanced instance is excluded from its own attached
    /// influence. Detached recovering entries continue to participate.
    #[must_use]
    pub fn category_gain(
        &self,
        category: SoundCategory,
        excluded_source: Option<AdvancedSoundInstanceId>,
    ) -> f32 {
        let Some(category_index) = category_index(category) else {
            return 1.0;
        };
        self.influences
            .iter()
            .filter(|influence| influence.source != excluded_source || influence.source.is_none())
            .map(|influence| influence.current[category_index])
            .fold(1.0, f32::min)
    }

    /// Returns the number of attached and recovering stock influence entries.
    #[must_use]
    pub fn influence_count(&self) -> usize {
        self.influences.len()
    }

    /// Reports whether an advanced instance currently owns an attached entry.
    #[must_use]
    pub fn contains_source(&self, source: AdvancedSoundInstanceId) -> bool {
        self.influences
            .iter()
            .any(|influence| influence.source == Some(source))
    }
}

/// One stock `SOUND_LOOP` transition node.
#[derive(Debug)]
struct DuckInfluence {
    source: Option<AdvancedSoundInstanceId>,
    targets: [f32; 3],
    current: [f32; 3],
    influence_radii: [f32; 2],
    transition_milliseconds: [i32; 2],
    latched: bool,
}

impl DuckInfluence {
    /// Preserves the stock per-category loop, including same-tick detachment.
    fn update(
        &mut self,
        elapsed_milliseconds: i32,
        listener_position: [f32; 3],
        emitter_position: Option<[f32; 3]>,
    ) {
        for category_index in 0..self.current.len() {
            let Some(_source) = self.source else {
                recover_gain(
                    &mut self.current[category_index],
                    elapsed_milliseconds,
                    self.transition_milliseconds[1],
                );
                continue;
            };
            let Some(emitter_position) = emitter_position else {
                self.source = None;
                recover_gain(
                    &mut self.current[category_index],
                    elapsed_milliseconds,
                    self.transition_milliseconds[1],
                );
                continue;
            };

            let distance = squared_distance(listener_position, emitter_position).sqrt();
            let position_is_origin = emitter_position == [0.0; 3];
            let [inner_radius, outer_radius] = self.influence_radii;
            if position_is_origin
                || distance < inner_radius
                || (distance < outer_radius && self.latched)
            {
                duck_gain(
                    &mut self.current[category_index],
                    self.targets[category_index],
                    elapsed_milliseconds,
                    self.transition_milliseconds[0],
                );
                self.latched = true;
            } else {
                // Stock detaches during the first category iteration. Later
                // categories in this same tick observe a recovering entry.
                self.latched = false;
                self.source = None;
            }
        }
    }
}

/// Moves one category toward its authored target at stock's fixed unit slope.
fn duck_gain(current: &mut f32, target: f32, elapsed_milliseconds: i32, duration: i32) {
    if *current == target {
        return;
    }
    if duration < 1 {
        *current = target;
        return;
    }

    *current -= elapsed_milliseconds as f32 / duration as f32;
    if *current < target {
        *current = target;
    }
}

/// Moves one detached category back toward neutral at stock's unit slope.
fn recover_gain(current: &mut f32, elapsed_milliseconds: i32, duration: i32) {
    if *current == 1.0 {
        return;
    }
    if duration < 1 {
        *current = 1.0;
        return;
    }

    *current += elapsed_milliseconds as f32 / duration as f32;
    if 1.0 < *current {
        *current = 1.0;
    }
}

/// Maps stock category discriminators to the node's SFX/music/ambience order.
const fn category_index(category: SoundCategory) -> Option<usize> {
    match category {
        SoundCategory::Sfx => Some(0),
        SoundCategory::Music => Some(1),
        SoundCategory::Ambience => Some(2),
        SoundCategory::Cinematic | SoundCategory::ScriptSound | SoundCategory::RacialCinematic => {
            None
        }
    }
}

/// Computes the same full XYZ metric used by the executable.
fn squared_distance(first: [f32; 3], second: [f32; 3]) -> f32 {
    let x = first[0] - second[0];
    let y = first[1] - second[1];
    let z = first[2] - second[2];
    x * x + y * y + z * z
}
