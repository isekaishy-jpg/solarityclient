//! Owned build-12340 sequence, skeleton, and nested bone-track decoding.

use glam::{Quat, Vec3};

use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath, AssetStore};

mod camera;
mod event;
mod light;
mod material;
mod particle;
mod ribbon;

pub use camera::M2Camera;
pub use event::{M2Event, M2EventTrack};
pub use light::{M2Light, M2LightKind};
pub use material::{M2ColorAnimation, M2TextureTransform, M2TextureWeight};
pub use particle::{M2ParticleEmitter, M2ParticleLifetimeTrack};
pub use ribbon::M2RibbonEmitter;

/// Stock interpolation operation authored by one M2 track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2Interpolation {
    /// Hold the preceding key.
    Step,
    /// Interpolate directly between adjacent values.
    Linear,
    /// Use the incoming and outgoing Bezier control points stored with each key.
    Bezier,
    /// Use the incoming and outgoing Hermite tangents stored with each key.
    Hermite,
}

impl M2Interpolation {
    /// Converts the version-264 selector without substituting unknown values.
    fn from_raw(path: &AssetPath, value: u16, field: &str) -> Result<Self, AssetError> {
        match value {
            0 => Ok(Self::Step),
            1 => Ok(Self::Linear),
            2 => Ok(Self::Bezier),
            3 => Ok(Self::Hermite),
            _ => Err(model_decode(
                path,
                format!("{field} has unsupported interpolation {value}"),
            )),
        }
    }

    /// Returns the stored values belonging to each timestamp.
    const fn values_per_key(self) -> usize {
        match self {
            Self::Step | Self::Linear => 1,
            Self::Bezier | Self::Hermite => 3,
        }
    }
}

/// One timestamp/value channel selected by a sequence or global clock.
#[derive(Clone, Debug, PartialEq)]
pub struct M2TrackChannel<T> {
    timestamps_ms: Vec<u32>,
    values: Vec<T>,
}

impl<T> M2TrackChannel<T> {
    /// Returns ordered key timestamps in milliseconds.
    #[must_use]
    pub fn timestamps_ms(&self) -> &[u32] {
        &self.timestamps_ms
    }

    /// Returns one value per key, or three consecutive values for spline tracks.
    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.values
    }
}

/// Every per-sequence channel for one animated M2 property.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Track<T> {
    interpolation: M2Interpolation,
    global_sequence: Option<u16>,
    channels: Vec<M2TrackChannel<T>>,
}

impl<T> M2Track<T> {
    /// Returns the authored interpolation operation.
    #[must_use]
    pub const fn interpolation(&self) -> M2Interpolation {
        self.interpolation
    }

    /// Returns the global-sequence clock index, or `None` for animation time.
    #[must_use]
    pub const fn global_sequence(&self) -> Option<u16> {
        self.global_sequence
    }

    /// Returns channels in their exact outer-array order.
    #[must_use]
    pub fn channels(&self) -> &[M2TrackChannel<T>] {
        &self.channels
    }
}

/// Where stock stores the keyframe payload for an animation sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2SequenceStorage {
    /// Track arrays point into the M2 body.
    Internal,
    /// Track arrays point into `Model####-##.anim`.
    External,
    /// The sequence delegates to another sequence record.
    Alias,
}

/// One exact 64-byte version-264 animation-sequence record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2Sequence {
    animation_id: u16,
    variation_index: u16,
    duration_ms: u32,
    movement_speed: f32,
    flags: u32,
    frequency: i16,
    replay_range: (u32, u32),
    blend_time_ms: u32,
    bounds: (Vec3, Vec3, f32),
    variation_next: Option<u16>,
    alias_next: Option<u16>,
    storage: M2SequenceStorage,
}

impl M2Sequence {
    /// Returns the AnimationData identifier selected by gameplay state.
    #[must_use]
    pub const fn animation_id(self) -> u16 {
        self.animation_id
    }
    /// Returns the authored variation number used in companion filenames.
    #[must_use]
    pub const fn variation_index(self) -> u16 {
        self.variation_index
    }
    /// Returns the authored cycle duration in milliseconds.
    #[must_use]
    pub const fn duration_ms(self) -> u32 {
        self.duration_ms
    }
    /// Returns the movement speed associated with this sequence.
    #[must_use]
    pub const fn movement_speed(self) -> f32 {
        self.movement_speed
    }
    /// Returns all sequence flags, including behavior not yet consumed.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }
    /// Returns the signed selection frequency.
    #[must_use]
    pub const fn frequency(self) -> i16 {
        self.frequency
    }
    /// Returns the authored minimum and exclusive-maximum cycle-count inputs.
    #[must_use]
    pub const fn replay_range(self) -> (u32, u32) {
        self.replay_range
    }
    /// Scales one CRT `rand` result into stock's total sequence cycle count.
    ///
    /// `CM2Model` multiplies the 15-bit roll by `maximum - minimum`, shifts
    /// right by 15, and adds the minimum. A zero result is promoted to one so
    /// every admitted sequence plays at least once.
    #[must_use]
    pub fn cycle_count(self, random_roll: u16) -> u32 {
        debug_assert!(random_roll <= 0x7fff);
        let (minimum, maximum) = self.replay_range;
        let minimum = minimum as i32;
        let maximum = maximum as i32;
        let scaled = i32::from(random_roll).wrapping_mul(maximum.wrapping_sub(minimum)) / 32_768;
        let cycles = minimum.wrapping_add(scaled);
        if cycles == 0 { 1 } else { cycles as u32 }
    }
    /// Returns the stock transition duration in milliseconds.
    #[must_use]
    pub const fn blend_time_ms(self) -> u32 {
        self.blend_time_ms
    }
    /// Returns the sequence bounding box and sphere radius.
    #[must_use]
    pub const fn bounds(self) -> (Vec3, Vec3, f32) {
        self.bounds
    }
    /// Returns the next variation record in the authored chain.
    #[must_use]
    pub const fn variation_next(self) -> Option<u16> {
        self.variation_next
    }
    /// Returns the delegated record for an alias sequence.
    #[must_use]
    pub const fn alias_next(self) -> Option<u16> {
        self.alias_next
    }
    /// Returns whether keyframes are internal, external, or delegated.
    #[must_use]
    pub const fn storage(self) -> M2SequenceStorage {
        self.storage
    }
}

/// One decoded bone and its three animation tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Bone {
    key_bone_id: i32,
    flags: u32,
    parent: Option<u16>,
    submesh_id: u16,
    name_crc: u32,
    translation: M2Track<Vec3>,
    rotation: M2Track<Quat>,
    scale: M2Track<Vec3>,
    pivot: Vec3,
}

impl M2Bone {
    /// Returns the key-bone semantic ID, with `-1` meaning no semantic role.
    #[must_use]
    pub const fn key_bone_id(&self) -> i32 {
        self.key_bone_id
    }
    /// Returns all authored bone flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
    /// Returns the parent index, or `None` for a skeleton root.
    #[must_use]
    pub const fn parent(&self) -> Option<u16> {
        self.parent
    }
    /// Returns the authored submesh association.
    #[must_use]
    pub const fn submesh_id(&self) -> u16 {
        self.submesh_id
    }
    /// Returns the debugging/name CRC embedded in version-264 bones.
    #[must_use]
    pub const fn name_crc(&self) -> u32 {
        self.name_crc
    }
    /// Returns local translation keyframes.
    #[must_use]
    pub const fn translation(&self) -> &M2Track<Vec3> {
        &self.translation
    }
    /// Returns compressed-and-expanded local rotation keyframes.
    #[must_use]
    pub const fn rotation(&self) -> &M2Track<Quat> {
        &self.rotation
    }
    /// Returns local scale keyframes.
    #[must_use]
    pub const fn scale(&self) -> &M2Track<Vec3> {
        &self.scale
    }
    /// Returns the model-space pivot around which local tracks compose.
    #[must_use]
    pub const fn pivot(&self) -> Vec3 {
        self.pivot
    }
}

/// Owned sequence catalog and bone animation data for one decoded M2.
#[derive(Debug)]
pub struct M2AnimationSet {
    global_sequence_durations_ms: Vec<u32>,
    sequences: Vec<M2Sequence>,
    animation_lookup: Vec<u16>,
    sequence_available: Vec<bool>,
    bones: Vec<M2Bone>,
    colors: Vec<M2ColorAnimation>,
    texture_weights: Vec<M2TextureWeight>,
    texture_transforms: Vec<M2TextureTransform>,
    cameras: Vec<M2Camera>,
    camera_lookup: Vec<Option<u16>>,
    events: Vec<M2Event>,
    lights: Vec<M2Light>,
    ribbons: Vec<M2RibbonEmitter>,
    particles: Vec<M2ParticleEmitter>,
}

impl M2AnimationSet {
    /// Loads external sequence payloads and decodes every bone track.
    pub(super) fn load(
        store: &mut AssetStore,
        model_path: &AssetPath,
        model_bytes: &[u8],
    ) -> Result<Self, AssetError> {
        let globals = decode_global_sequences(model_path, model_bytes)?;
        let sequences = decode_sequences(model_path, model_bytes)?;
        let animation_lookup = decode_animation_lookup(model_path, model_bytes, sequences.len())?;
        validate_aliases(model_path, &sequences)?;
        validate_variations(model_path, &sequences)?;
        let mut payloads = Vec::with_capacity(sequences.len());
        let mut available = Vec::with_capacity(sequences.len());
        for sequence in &sequences {
            if sequence.storage != M2SequenceStorage::External {
                payloads.push(None);
                available.push(true);
                continue;
            }
            let path = external_animation_path(
                model_path,
                sequence.animation_id,
                sequence.variation_index,
            )?;
            if !store.contains(&path)? {
                payloads.push(None);
                available.push(false);
                continue;
            }
            payloads.push(Some((path.clone(), store.read(&path)?.into_bytes())));
            available.push(true);
        }
        // Aliases have no payload and inherit availability from their final
        // non-alias target, which may itself require a missing companion.
        for index in 0..sequences.len() {
            let mut target = index;
            while let Some(next) = sequences[target].alias_next {
                target = usize::from(next);
            }
            if target != index {
                available[index] = available[target];
            }
        }
        let bones = decode_bones(model_path, model_bytes, &globals, &sequences, &payloads)?;
        let colors = decode_colors(model_path, model_bytes, &globals, &sequences, &payloads)?;
        let texture_weights =
            decode_texture_weights(model_path, model_bytes, &globals, &sequences, &payloads)?;
        let texture_transforms =
            decode_texture_transforms(model_path, model_bytes, &globals, &sequences, &payloads)?;
        let (cameras, camera_lookup) =
            camera::decode_cameras(model_path, model_bytes, &globals, &sequences, &payloads)?;
        let events = event::decode_events(
            model_path,
            model_bytes,
            &globals,
            &sequences,
            &payloads,
            bones.len(),
        )?;
        let lights = light::decode_lights(
            model_path,
            model_bytes,
            &globals,
            &sequences,
            &payloads,
            bones.len(),
        )?;
        let ribbons = ribbon::decode_ribbons(
            model_path,
            model_bytes,
            &globals,
            &sequences,
            &payloads,
            bones.len(),
        )?;
        let particles = particle::decode_particles(
            model_path,
            model_bytes,
            &globals,
            &sequences,
            &payloads,
            bones.len(),
        )?;
        Ok(Self {
            global_sequence_durations_ms: globals,
            sequences,
            animation_lookup,
            sequence_available: available,
            bones,
            colors,
            texture_weights,
            texture_transforms,
            cameras,
            camera_lookup,
            events,
            lights,
            ribbons,
            particles,
        })
    }

    /// Returns global clock periods in exact table order.
    #[must_use]
    pub fn global_sequence_durations_ms(&self) -> &[u32] {
        &self.global_sequence_durations_ms
    }
    /// Returns every animation sequence in file order.
    #[must_use]
    pub fn sequences(&self) -> &[M2Sequence] {
        &self.sequences
    }
    /// Returns the exact build-12340 animation-ID lookup buckets.
    #[must_use]
    pub fn animation_lookup(&self) -> &[u16] {
        &self.animation_lookup
    }
    /// Reports whether the sequence's required payload is available.
    #[must_use]
    pub fn is_sequence_available(&self, index: usize) -> Option<bool> {
        self.sequence_available.get(index).copied()
    }

    /// Resolves an alias chain to the sequence that owns its track channels.
    #[must_use]
    pub fn resolve_sequence_alias(&self, index: usize) -> Option<usize> {
        let mut current = index;
        loop {
            let sequence = self.sequences.get(current)?;
            let Some(next) = sequence.alias_next else {
                return Some(current);
            };
            current = usize::from(next);
        }
    }

    /// Selects one available sequence through stock's lookup and variation chain.
    ///
    /// `preferred_variation` reproduces the initial primary-variation request.
    /// If that variation is absent, `weighted_roll` is the raw 15-bit SRand
    /// result consumed against positive authored frequencies. No whole-array
    /// scan repairs a missing lookup or broken variation chain.
    #[must_use]
    pub fn select_sequence(
        &self,
        animation_id: u16,
        preferred_variation: Option<u16>,
        weighted_roll: u32,
    ) -> Option<usize> {
        let first = self.lookup_sequence(animation_id)?;
        if let Some(preferred) = preferred_variation {
            let mut current = Some(first);
            let mut remaining = self.sequences.len();
            while let Some(index) = current {
                let sequence = self.sequences.get(index)?;
                if sequence.animation_id != animation_id || remaining == 0 {
                    return None;
                }
                if sequence.variation_index == preferred
                    && self.sequence_available.get(index) == Some(&true)
                {
                    return Some(index);
                }
                current = sequence.variation_next.map(usize::from);
                remaining -= 1;
            }
        }

        let mut available_count = 0_u64;
        let mut total_weight = 0_u64;
        self.visit_variations(first, animation_id, |index, sequence| {
            if self.sequence_available[index] {
                available_count += 1;
                total_weight += u64::from(sequence.frequency.max(0) as u16);
            }
        })?;
        if available_count == 0 {
            return None;
        }
        let mut roll = u64::from(weighted_roll);
        let mut zero_weight_slot = if total_weight == 0 {
            Some(roll % available_count)
        } else {
            None
        };
        let mut selected = None;
        self.visit_variations(first, animation_id, |index, sequence| {
            if selected.is_some() || !self.sequence_available[index] {
                return;
            }
            if let Some(slot) = zero_weight_slot.as_mut() {
                if *slot == 0 {
                    selected = Some(index);
                } else {
                    *slot -= 1;
                }
                return;
            }
            let weight = u64::from(sequence.frequency.max(0) as u16);
            if roll < weight {
                selected = Some(index);
            } else {
                roll -= weight;
            }
        })?;
        // Stock retains the base available sequence when incomplete authored
        // frequencies do not consume the raw SRand range.
        selected.or_else(|| {
            self.visit_variations(first, animation_id, |index, _sequence| {
                if selected.is_none() && self.sequence_available[index] {
                    selected = Some(index);
                }
            })?;
            selected
        })
    }

    /// Finds one exact available variation without consuming a weighted roll.
    #[must_use]
    pub fn sequence_for_variation(&self, animation_id: u16, variation_index: u16) -> Option<usize> {
        let first = self.lookup_sequence(animation_id)?;
        let mut selected = None;
        self.visit_variations(first, animation_id, |index, sequence| {
            if selected.is_none()
                && sequence.variation_index == variation_index
                && self.sequence_available[index]
            {
                selected = Some(index);
            }
        })?;
        selected
    }

    /// Counts available records in one authored animation variation chain.
    #[must_use]
    pub fn available_variation_count(&self, animation_id: u16) -> Option<usize> {
        let first = self.lookup_sequence(animation_id)?;
        let mut count = 0;
        self.visit_variations(first, animation_id, |index, _sequence| {
            count += usize::from(self.sequence_available[index]);
        })?;
        Some(count)
    }

    /// Probes the on-disk quadratic lookup without a compatibility scan.
    fn lookup_sequence(&self, animation_id: u16) -> Option<usize> {
        if self.animation_lookup.is_empty() {
            // Build 12340 scans the sequence records only when the M2 omits
            // the lookup table entirely. A present table that misses retains
            // that miss and never enters this path.
            return self
                .sequences
                .iter()
                .position(|sequence| sequence.animation_id == animation_id);
        }
        let mut bucket = usize::from(animation_id) % self.animation_lookup.len();
        for stride in 1..=self.animation_lookup.len() {
            let index = *self.animation_lookup.get(bucket)?;
            if index == u16::MAX {
                return None;
            }
            let index = usize::from(index);
            if self.sequences.get(index)?.animation_id == animation_id {
                return Some(index);
            }
            bucket = (bucket + stride * stride) % self.animation_lookup.len();
        }
        None
    }

    /// Visits one closed, already validated same-ID variation chain.
    fn visit_variations(
        &self,
        first: usize,
        animation_id: u16,
        mut visitor: impl FnMut(usize, &M2Sequence),
    ) -> Option<()> {
        let mut current = Some(first);
        let mut remaining = self.sequences.len();
        while let Some(index) = current {
            let sequence = self.sequences.get(index)?;
            if sequence.animation_id != animation_id || remaining == 0 {
                return None;
            }
            visitor(index, sequence);
            current = sequence.variation_next.map(usize::from);
            remaining -= 1;
        }
        Some(())
    }
    /// Returns the validated parent-linked model skeleton.
    #[must_use]
    pub fn bones(&self) -> &[M2Bone] {
        &self.bones
    }

    /// Returns animated mesh colors in exact M2 table order.
    #[must_use]
    pub fn colors(&self) -> &[M2ColorAnimation] {
        &self.colors
    }

    /// Returns animated texture opacity multipliers in table order.
    #[must_use]
    pub fn texture_weights(&self) -> &[M2TextureWeight] {
        &self.texture_weights
    }

    /// Returns animated texture transforms in exact M2 table order.
    #[must_use]
    pub fn texture_transforms(&self) -> &[M2TextureTransform] {
        &self.texture_transforms
    }

    /// Returns authored model cameras in exact M2 table order.
    #[must_use]
    pub fn cameras(&self) -> &[M2Camera] {
        &self.cameras
    }

    /// Returns semantic camera slots, retaining absent `-1` entries.
    #[must_use]
    pub fn camera_lookup(&self) -> &[Option<u16>] {
        &self.camera_lookup
    }

    /// Returns authored model events in exact M2 table order.
    #[must_use]
    pub fn events(&self) -> &[M2Event] {
        &self.events
    }

    /// Returns authored model lights in exact M2 table order.
    #[must_use]
    pub fn lights(&self) -> &[M2Light] {
        &self.lights
    }

    /// Returns authored ribbon emitters in exact M2 table order.
    #[must_use]
    pub fn ribbons(&self) -> &[M2RibbonEmitter] {
        &self.ribbons
    }

    /// Returns authored particle emitters in exact M2 table order.
    #[must_use]
    pub fn particles(&self) -> &[M2ParticleEmitter] {
        &self.particles
    }

    /// Returns the number of model bones without exposing dependency storage.
    #[must_use]
    pub const fn bone_count(&self) -> usize {
        self.bones.len()
    }
}

/// One little-endian count/offset pair retained by WotLK nested tracks.
#[derive(Clone, Copy)]
struct ArrayRef {
    count: usize,
    offset: usize,
}

/// Decodes the top-level global-sequence duration table.
fn decode_global_sequences(path: &AssetPath, bytes: &[u8]) -> Result<Vec<u32>, AssetError> {
    let array = array_ref(path, bytes, 0x14, "global sequences")?;
    validate_array(path, bytes, array, 4, "global sequences")?;
    (0..array.count)
        .map(|index| read_u32(path, bytes, array.offset + index * 4, "global sequence"))
        .collect()
}

/// Decodes and validates every 64-byte WotLK sequence record.
fn decode_sequences(path: &AssetPath, bytes: &[u8]) -> Result<Vec<M2Sequence>, AssetError> {
    let array = array_ref(path, bytes, 0x1c, "sequences")?;
    validate_array(path, bytes, array, 64, "sequences")?;
    let mut result = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 64;
        let flags = read_u32(path, bytes, offset + 12, "sequence flags")?;
        let storage = sequence_storage(flags);
        let variation_raw = read_i16(path, bytes, offset + 60, "sequence variation")?;
        let variation_next =
            checked_sequence_ref(path, index, variation_raw, array.count, "variation")?;
        let alias_raw = read_u16(path, bytes, offset + 62, "sequence alias")?;
        let alias_next = if storage == M2SequenceStorage::Alias {
            if usize::from(alias_raw) >= array.count {
                return Err(model_decode(
                    path,
                    format!("sequence {index} alias is missing"),
                ));
            }
            Some(alias_raw)
        } else {
            None
        };
        result.push(M2Sequence {
            animation_id: read_u16(path, bytes, offset, "sequence animation ID")?,
            variation_index: read_u16(path, bytes, offset + 2, "sequence variation index")?,
            duration_ms: read_u32(path, bytes, offset + 4, "sequence duration")?,
            movement_speed: read_f32(path, bytes, offset + 8, "sequence speed")?,
            flags,
            frequency: read_i16(path, bytes, offset + 16, "sequence frequency")?,
            replay_range: (
                read_u32(path, bytes, offset + 20, "sequence replay minimum")?,
                read_u32(path, bytes, offset + 24, "sequence replay maximum")?,
            ),
            blend_time_ms: read_u32(path, bytes, offset + 28, "sequence blend time")?,
            bounds: (
                read_vec3(path, bytes, offset + 32, "sequence minimum bounds")?,
                read_vec3(path, bytes, offset + 44, "sequence maximum bounds")?,
                read_f32(path, bytes, offset + 56, "sequence bounds radius")?,
            ),
            variation_next,
            alias_next,
            storage,
        });
    }
    Ok(result)
}

/// Decodes the authoritative animation-ID lookup and validates record indices.
fn decode_animation_lookup(
    path: &AssetPath,
    bytes: &[u8],
    sequence_count: usize,
) -> Result<Vec<u16>, AssetError> {
    let array = array_ref(path, bytes, 0x24, "animation lookup")?;
    validate_array(path, bytes, array, 2, "animation lookup")?;
    let mut result = Vec::with_capacity(array.count);
    for bucket in 0..array.count {
        let index = read_u16(path, bytes, array.offset + bucket * 2, "animation lookup")?;
        if index != u16::MAX && usize::from(index) >= sequence_count {
            return Err(model_decode(
                path,
                format!("animation lookup bucket {bucket} references missing sequence {index}"),
            ));
        }
        result.push(index);
    }
    Ok(result)
}

/// Converts a signed optional sequence reference and validates positive values.
fn checked_sequence_ref(
    path: &AssetPath,
    index: usize,
    value: i16,
    count: usize,
    label: &str,
) -> Result<Option<u16>, AssetError> {
    if value < 0 {
        return Ok(None);
    }
    let value = value as u16;
    if usize::from(value) >= count {
        return Err(model_decode(
            path,
            format!("sequence {index} {label} is missing"),
        ));
    }
    Ok(Some(value))
}

/// Rejects alias cycles before runtime selection can loop forever.
fn validate_aliases(path: &AssetPath, sequences: &[M2Sequence]) -> Result<(), AssetError> {
    for start in 0..sequences.len() {
        let mut visited = vec![false; sequences.len()];
        let mut current = start;
        while let Some(next) = sequences[current].alias_next {
            if visited[current] {
                return Err(model_decode(
                    path,
                    format!("sequence {start} alias chain cycles"),
                ));
            }
            visited[current] = true;
            current = usize::from(next);
        }
    }
    Ok(())
}

/// Rejects cross-animation links and cycles before selection enters the hot path.
fn validate_variations(path: &AssetPath, sequences: &[M2Sequence]) -> Result<(), AssetError> {
    for start in 0..sequences.len() {
        let animation_id = sequences[start].animation_id;
        let mut visited = vec![false; sequences.len()];
        let mut current = start;
        while let Some(next) = sequences[current].variation_next {
            if visited[current] {
                return Err(model_decode(
                    path,
                    format!("sequence {start} variation chain cycles"),
                ));
            }
            visited[current] = true;
            current = usize::from(next);
            if sequences[current].animation_id != animation_id {
                return Err(model_decode(
                    path,
                    format!("sequence {start} variation chain crosses animation ID {animation_id}"),
                ));
            }
        }
    }
    Ok(())
}

/// Decodes the exact 88-byte bones and their nested channels.
fn decode_bones(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<Vec<M2Bone>, AssetError> {
    let array = array_ref(path, bytes, 0x2c, "bones")?;
    validate_array(path, bytes, array, 88, "bones")?;
    let mut result = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 88;
        let parent_raw = read_i16(path, bytes, offset + 8, "bone parent")?;
        let parent = if parent_raw < 0 {
            None
        } else {
            let value = parent_raw as u16;
            if usize::from(value) >= array.count {
                return Err(model_decode(
                    path,
                    format!("bone {index} parent is missing"),
                ));
            }
            Some(value)
        };
        result.push(M2Bone {
            key_bone_id: read_i32(path, bytes, offset, "bone key ID")?,
            flags: read_u32(path, bytes, offset + 4, "bone flags")?,
            parent,
            submesh_id: read_u16(path, bytes, offset + 10, "bone submesh ID")?,
            name_crc: read_u32(path, bytes, offset + 12, "bone name CRC")?,
            translation: decode_track(
                path,
                bytes,
                offset + 16,
                &format!("bone {index} translation"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            rotation: decode_track(
                path,
                bytes,
                offset + 36,
                &format!("bone {index} rotation"),
                globals,
                sequences,
                payloads,
                8,
                decode_quaternion,
            )?,
            scale: decode_track(
                path,
                bytes,
                offset + 56,
                &format!("bone {index} scale"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            pivot: read_vec3(path, bytes, offset + 76, "bone pivot")?,
        });
    }
    validate_bone_hierarchy(path, &result)?;
    Ok(result)
}

/// Decodes the 40-byte RGB/fixed16-alpha material animation records.
fn decode_colors(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<Vec<M2ColorAnimation>, AssetError> {
    let array = array_ref(path, bytes, 0x48, "colors")?;
    validate_array(path, bytes, array, 40, "colors")?;
    let mut result = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 40;
        result.push(M2ColorAnimation::new(
            decode_track(
                path,
                bytes,
                offset,
                &format!("color {index} RGB"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            decode_track(
                path,
                bytes,
                offset + 20,
                &format!("color {index} alpha"),
                globals,
                sequences,
                payloads,
                2,
                decode_fixed16,
            )?,
        ));
    }
    Ok(result)
}

/// Decodes the 20-byte fixed16 texture-weight animation records.
fn decode_texture_weights(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<Vec<M2TextureWeight>, AssetError> {
    let array = array_ref(path, bytes, 0x58, "texture weights")?;
    validate_array(path, bytes, array, 20, "texture weights")?;
    let mut result = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 20;
        result.push(M2TextureWeight::new(decode_track(
            path,
            bytes,
            offset,
            &format!("texture weight {index}"),
            globals,
            sequences,
            payloads,
            2,
            decode_fixed16,
        )?));
    }
    Ok(result)
}

/// Decodes the 60-byte translation/rotation/scale texture records.
fn decode_texture_transforms(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<Vec<M2TextureTransform>, AssetError> {
    let array = array_ref(path, bytes, 0x60, "texture transforms")?;
    validate_array(path, bytes, array, 60, "texture transforms")?;
    let mut result = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 60;
        result.push(M2TextureTransform::new(
            decode_track(
                path,
                bytes,
                offset,
                &format!("texture transform {index} translation"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            decode_track(
                path,
                bytes,
                offset + 20,
                &format!("texture transform {index} rotation"),
                globals,
                sequences,
                payloads,
                8,
                decode_quaternion,
            )?,
            decode_track(
                path,
                bytes,
                offset + 40,
                &format!("texture transform {index} scale"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
        ));
    }
    Ok(result)
}

/// Rejects parent cycles while allowing valid parent records in any order.
fn validate_bone_hierarchy(path: &AssetPath, bones: &[M2Bone]) -> Result<(), AssetError> {
    for start in 0..bones.len() {
        let mut visited = vec![false; bones.len()];
        let mut current = Some(start);
        while let Some(index) = current {
            if visited[index] {
                return Err(model_decode(
                    path,
                    format!("bone {start} parent chain cycles"),
                ));
            }
            visited[index] = true;
            current = bones[index].parent.map(usize::from);
        }
    }
    Ok(())
}

/// Expands one WotLK nested track using each sequence's actual data source.
#[allow(clippy::too_many_arguments)]
fn decode_track<T>(
    path: &AssetPath,
    model_bytes: &[u8],
    offset: usize,
    field: &str,
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    value_size: usize,
    decode_value: fn(&AssetPath, &[u8], usize, &str) -> Result<T, AssetError>,
) -> Result<M2Track<T>, AssetError> {
    let interpolation =
        M2Interpolation::from_raw(path, read_u16(path, model_bytes, offset, field)?, field)?;
    let global_raw = read_i16(path, model_bytes, offset + 2, field)?;
    let global_sequence = if global_raw < 0 {
        None
    } else {
        let value = global_raw as u16;
        if usize::from(value) >= globals.len() {
            return Err(model_decode(
                path,
                format!("{field} global sequence is missing"),
            ));
        }
        Some(value)
    };
    let timestamp_arrays = array_ref(path, model_bytes, offset + 4, field)?;
    let value_arrays = array_ref(path, model_bytes, offset + 12, field)?;
    validate_array(path, model_bytes, timestamp_arrays, 8, field)?;
    validate_array(path, model_bytes, value_arrays, 8, field)?;
    if timestamp_arrays.count != value_arrays.count {
        return Err(model_decode(path, format!("{field} channel counts differ")));
    }
    let mut channels = Vec::with_capacity(timestamp_arrays.count);
    for channel_index in 0..timestamp_arrays.count {
        if global_sequence.is_none()
            && sequences
                .get(channel_index)
                .is_some_and(|value| value.storage == M2SequenceStorage::Alias)
        {
            channels.push(empty_channel());
            continue;
        }
        let payload = if global_sequence.is_none()
            && sequences
                .get(channel_index)
                .is_some_and(|value| value.storage == M2SequenceStorage::External)
        {
            let Some((payload_path, payload_bytes)) =
                payloads.get(channel_index).and_then(Option::as_ref)
            else {
                channels.push(empty_channel());
                continue;
            };
            (payload_path, payload_bytes.as_slice())
        } else {
            (path, model_bytes)
        };
        let timestamps = array_ref(
            path,
            model_bytes,
            timestamp_arrays.offset + channel_index * 8,
            field,
        )?;
        let values = array_ref(
            path,
            model_bytes,
            value_arrays.offset + channel_index * 8,
            field,
        )?;
        validate_array(payload.0, payload.1, timestamps, 4, field)?;
        if values.count != timestamps.count {
            return Err(model_decode(
                payload.0,
                format!("{field} key counts differ"),
            ));
        }
        let stored_count = timestamps
            .count
            .checked_mul(interpolation.values_per_key())
            .ok_or_else(|| model_decode(payload.0, format!("{field} value count overflows")))?;
        validate_array(
            payload.0,
            payload.1,
            ArrayRef {
                count: stored_count,
                offset: values.offset,
            },
            value_size,
            field,
        )?;
        let mut timestamps_ms = Vec::with_capacity(timestamps.count);
        for index in 0..timestamps.count {
            timestamps_ms.push(read_u32(
                payload.0,
                payload.1,
                timestamps.offset + index * 4,
                field,
            )?);
        }
        if !timestamps_ms.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(model_decode(
                payload.0,
                format!("{field} timestamps are not ordered"),
            ));
        }
        let mut decoded_values = Vec::with_capacity(stored_count);
        for index in 0..stored_count {
            decoded_values.push(decode_value(
                payload.0,
                payload.1,
                values.offset + index * value_size,
                field,
            )?);
        }
        channels.push(M2TrackChannel {
            timestamps_ms,
            values: decoded_values,
        });
    }
    Ok(M2Track {
        interpolation,
        global_sequence,
        channels,
    })
}

/// Builds the explicit empty slot retained for aliases and unavailable payloads.
fn empty_channel<T>() -> M2TrackChannel<T> {
    M2TrackChannel {
        timestamps_ms: Vec::new(),
        values: Vec::new(),
    }
}

/// Decodes one finite vector key without the dependency's NaN substitution.
fn decode_vec3(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec3, AssetError> {
    read_vec3(path, bytes, offset, field)
}

/// Expands stock's signed 16-bit quaternion representation.
fn decode_quaternion(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Quat, AssetError> {
    let raw = [
        read_i16(path, bytes, offset, field)?,
        read_i16(path, bytes, offset + 2, field)?,
        read_i16(path, bytes, offset + 4, field)?,
        read_i16(path, bytes, offset + 6, field)?,
    ];
    let expanded = raw.map(|value| {
        let numerator = if value < 0 {
            f32::from(value) + 32768.0
        } else {
            f32::from(value) - 32767.0
        };
        numerator / 32767.0
    });
    let value = Quat::from_xyzw(expanded[0], expanded[1], expanded[2], expanded[3]);
    if !value.is_finite() || value.length_squared() <= 0.000_001 {
        return Err(model_decode(
            path,
            format!("{field} contains an invalid quaternion"),
        ));
    }
    Ok(value.normalize())
}

/// Normalizes stock's signed fixed16 material scalar by exactly `32767`.
fn decode_fixed16(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<f32, AssetError> {
    Ok(f32::from(read_i16(path, bytes, offset, field)?) / 32_767.0)
}

/// Reproduces the version-264 external companion naming rule.
fn external_animation_path(
    model_path: &AssetPath,
    animation_id: u16,
    variation_index: u16,
) -> Result<AssetPath, AssetError> {
    let stem = model_path
        .as_str()
        .strip_suffix(".M2")
        .ok_or_else(|| model_decode(model_path, "model path does not end in .m2".to_owned()))?;
    AssetPath::new(format!("{stem}{animation_id:04}-{variation_index:02}.anim"))
}

/// Maps the exact flag test recovered from build 12340.
const fn sequence_storage(flags: u32) -> M2SequenceStorage {
    if flags & 0x40 != 0 {
        M2SequenceStorage::Alias
    } else if flags & 0x130 == 0 {
        M2SequenceStorage::External
    } else {
        M2SequenceStorage::Internal
    }
}

/// Reads one count/offset pair.
fn array_ref(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<ArrayRef, AssetError> {
    Ok(ArrayRef {
        count: read_u32(path, bytes, offset, field)? as usize,
        offset: read_u32(path, bytes, offset + 4, field)? as usize,
    })
}

/// Validates one array with checked byte arithmetic.
fn validate_array(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
    stride: usize,
    field: &str,
) -> Result<(), AssetError> {
    let byte_count = array
        .count
        .checked_mul(stride)
        .ok_or_else(|| model_decode(path, format!("{field} byte count overflows")))?;
    let end = array
        .offset
        .checked_add(byte_count)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    if end > bytes.len() {
        return Err(model_decode(
            path,
            format!("{field} array exceeds its asset"),
        ));
    }
    Ok(())
}

/// Reads one finite vector.
fn read_vec3(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec3, AssetError> {
    let value = Vec3::new(
        read_f32(path, bytes, offset, field)?,
        read_f32(path, bytes, offset + 4, field)?,
        read_f32(path, bytes, offset + 8, field)?,
    );
    if !value.is_finite() {
        return Err(model_decode(
            path,
            format!("{field} contains a non-finite vector"),
        ));
    }
    Ok(value)
}

/// Reads one little-endian scalar.
fn read_u32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u32, AssetError> {
    Ok(u32::from_le_bytes(read_bytes::<4>(
        path, bytes, offset, field,
    )?))
}
/// Reads one signed little-endian scalar.
fn read_i32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<i32, AssetError> {
    Ok(i32::from_le_bytes(read_bytes::<4>(
        path, bytes, offset, field,
    )?))
}
/// Reads one little-endian word.
fn read_u16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u16, AssetError> {
    Ok(u16::from_le_bytes(read_bytes::<2>(
        path, bytes, offset, field,
    )?))
}
/// Reads one signed little-endian word.
fn read_i16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<i16, AssetError> {
    Ok(i16::from_le_bytes(read_bytes::<2>(
        path, bytes, offset, field,
    )?))
}
/// Reads one unsigned byte.
fn read_u8(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u8, AssetError> {
    Ok(read_bytes::<1>(path, bytes, offset, field)?[0])
}
/// Reads one signed byte.
fn read_i8(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<i8, AssetError> {
    Ok(read_bytes::<1>(path, bytes, offset, field)?[0] as i8)
}
/// Reads one finite little-endian float.
fn read_f32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<f32, AssetError> {
    let value = f32::from_le_bytes(read_bytes::<4>(path, bytes, offset, field)?);
    if !value.is_finite() {
        return Err(model_decode(
            path,
            format!("{field} contains a non-finite float"),
        ));
    }
    Ok(value)
}

/// Copies one fixed-width byte array without unchecked indexing.
fn read_bytes<const N: usize>(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<[u8; N], AssetError> {
    bytes
        .get(offset..offset + N)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| model_decode(path, format!("{field} is truncated")))
}
