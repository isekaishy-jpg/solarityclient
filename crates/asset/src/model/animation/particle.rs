//! Exact 476-byte build-12340 M2 particle-emitter decoding.

use glam::{Vec2, Vec3};

use super::{
    M2Sequence, M2Track, array_ref, decode_fixed16, decode_track, decode_vec3, read_f32, read_i16,
    read_u8, read_u16, read_u32, validate_array,
};
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// Build-12340 flag that packs three five-bit texture indices into `texture_id`.
const MULTI_TEXTURE_FLAG: u32 = 0x1000_0000;

/// One sequence-independent ramp sampled over a particle's normalized lifetime.
///
/// Unlike [`M2Track`], this WotLK `FBlock` has no interpolation selector or
/// global clock. Its `u16` timestamps cover the particle lifetime rather than
/// model-animation milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct M2ParticleLifetimeTrack<T> {
    timestamps: Vec<u16>,
    values: Vec<T>,
}

impl<T> M2ParticleLifetimeTrack<T> {
    /// Returns raw normalized-lifetime keys in authored order.
    #[must_use]
    pub fn timestamps(&self) -> &[u16] {
        &self.timestamps
    }

    /// Returns the value paired with each normalized-lifetime key.
    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.values
    }
}

/// One bone-attached particle generator and its complete WotLK parameter set.
#[derive(Clone, Debug, PartialEq)]
pub struct M2ParticleEmitter {
    id: u32,
    flags: u32,
    position: Vec3,
    bone_index: Option<u16>,
    texture_id: Option<u16>,
    geometry_model_path: Option<AssetPath>,
    child_emitter_model_path: Option<AssetPath>,
    blending_type: u8,
    emitter_type: u8,
    particle_color_index: u16,
    particle_type: u8,
    head_or_tail: u8,
    priority_plane: i16,
    texture_rows: u16,
    texture_columns: u16,
    emission_speed: M2Track<f32>,
    speed_variation: M2Track<f32>,
    vertical_range: M2Track<f32>,
    horizontal_range: M2Track<f32>,
    gravity: M2Track<f32>,
    lifespan: M2Track<f32>,
    lifespan_variation: f32,
    emission_rate: M2Track<f32>,
    emission_rate_variation: f32,
    emission_area_width: M2Track<f32>,
    emission_area_length: M2Track<f32>,
    z_source: M2Track<f32>,
    color: M2ParticleLifetimeTrack<Vec3>,
    alpha: M2ParticleLifetimeTrack<f32>,
    scale: M2ParticleLifetimeTrack<Vec2>,
    scale_variation: Vec2,
    head_uv_animation: M2ParticleLifetimeTrack<u16>,
    tail_uv_animation: M2ParticleLifetimeTrack<u16>,
    tail_length: f32,
    twinkle_speed: f32,
    twinkle_percent: f32,
    twinkle_scale: Vec2,
    inherit_velocity_scale: f32,
    drag: f32,
    base_spin: f32,
    base_spin_variation: f32,
    spin_speed: f32,
    spin_speed_variation: f32,
    tumble: (Vec3, Vec3),
    wind_vector: Vec3,
    wind_time: f32,
    follow_speed: (f32, f32),
    follow_scale: (f32, f32),
    spline_points: Vec<Vec3>,
    enabled: M2Track<u8>,
}

impl M2ParticleEmitter {
    /// Returns the authored emitter identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns all build-12340 particle behavior bits without narrowing them.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the emitter position relative to its owning bone.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the owning bone, or `None` for the `0xFFFF` sentinel.
    #[must_use]
    pub const fn bone_index(&self) -> Option<u16> {
        self.bone_index
    }

    /// Returns the raw texture field, or `None` for the `0xFFFF` sentinel.
    ///
    /// When [`Self::uses_multiple_textures`] is true, callers must consume the
    /// packed values returned by [`Self::texture_indices`] rather than treating
    /// this word as one table index.
    #[must_use]
    pub const fn texture_id(&self) -> Option<u16> {
        self.texture_id
    }

    /// Reports whether the raw texture word contains three five-bit indices.
    #[must_use]
    pub const fn uses_multiple_textures(&self) -> bool {
        self.flags & MULTI_TEXTURE_FLAG != 0
    }

    /// Expands the texture field into the exact indices consumed by the emitter.
    ///
    /// A normal emitter returns one populated slot. A multi-texture emitter
    /// returns all three five-bit slots. The array stays allocation-free because
    /// this is queried by resource validation and render preparation.
    #[must_use]
    pub const fn texture_indices(&self) -> [Option<u16>; 3] {
        let Some(texture_id) = self.texture_id else {
            return [None, None, None];
        };
        if self.uses_multiple_textures() {
            [
                Some(texture_id & 0x1f),
                Some((texture_id >> 5) & 0x1f),
                Some((texture_id >> 10) & 0x1f),
            ]
        } else {
            [Some(texture_id), None, None]
        }
    }

    /// Returns the optional model used for geometry particles.
    #[must_use]
    pub const fn geometry_model_path(&self) -> Option<&AssetPath> {
        self.geometry_model_path.as_ref()
    }

    /// Returns the optional model whose emitters are recursively spawned.
    #[must_use]
    pub const fn child_emitter_model_path(&self) -> Option<&AssetPath> {
        self.child_emitter_model_path.as_ref()
    }

    /// Returns the raw particle blend selector.
    #[must_use]
    pub const fn blending_type(&self) -> u8 {
        self.blending_type
    }

    /// Returns the authored generator shape selector.
    #[must_use]
    pub const fn emitter_type(&self) -> u8 {
        self.emitter_type
    }

    /// Returns the `ParticleColor.dbc` replacement-color selector.
    #[must_use]
    pub const fn particle_color_index(&self) -> u16 {
        self.particle_color_index
    }

    /// Returns the raw particle render-type byte retained by WotLK.
    #[must_use]
    pub const fn particle_type(&self) -> u8 {
        self.particle_type
    }

    /// Returns `0` for heads, `1` for tails, or `2` for both.
    #[must_use]
    pub const fn head_or_tail(&self) -> u8 {
        self.head_or_tail
    }

    /// Returns the signed scene priority plane.
    #[must_use]
    pub const fn priority_plane(&self) -> i16 {
        self.priority_plane
    }

    /// Returns the vertical flipbook cell count.
    #[must_use]
    pub const fn texture_rows(&self) -> u16 {
        self.texture_rows
    }

    /// Returns the horizontal flipbook cell count.
    #[must_use]
    pub const fn texture_columns(&self) -> u16 {
        self.texture_columns
    }

    /// Returns the animated base launch speed.
    #[must_use]
    pub const fn emission_speed(&self) -> &M2Track<f32> {
        &self.emission_speed
    }

    /// Returns the animated random launch-speed multiplier.
    #[must_use]
    pub const fn speed_variation(&self) -> &M2Track<f32> {
        &self.speed_variation
    }

    /// Returns the animated maximum polar launch angle.
    #[must_use]
    pub const fn vertical_range(&self) -> &M2Track<f32> {
        &self.vertical_range
    }

    /// Returns the animated maximum azimuth launch angle.
    #[must_use]
    pub const fn horizontal_range(&self) -> &M2Track<f32> {
        &self.horizontal_range
    }

    /// Returns animated gravity, including compressed values selected by flags.
    #[must_use]
    pub const fn gravity(&self) -> &M2Track<f32> {
        &self.gravity
    }

    /// Returns animated particle lifetime in seconds.
    #[must_use]
    pub const fn lifespan(&self) -> &M2Track<f32> {
        &self.lifespan
    }

    /// Returns the signed-random lifetime variation multiplier.
    #[must_use]
    pub const fn lifespan_variation(&self) -> f32 {
        self.lifespan_variation
    }

    /// Returns animated particles emitted per second.
    #[must_use]
    pub const fn emission_rate(&self) -> &M2Track<f32> {
        &self.emission_rate
    }

    /// Returns the signed-random per-update emission-rate variation.
    #[must_use]
    pub const fn emission_rate_variation(&self) -> f32 {
        self.emission_rate_variation
    }

    /// Returns animated plane width or sphere maximum radius.
    #[must_use]
    pub const fn emission_area_width(&self) -> &M2Track<f32> {
        &self.emission_area_width
    }

    /// Returns animated plane length or sphere minimum radius.
    #[must_use]
    pub const fn emission_area_length(&self) -> &M2Track<f32> {
        &self.emission_area_length
    }

    /// Returns the animated source height used to aim initial velocity.
    #[must_use]
    pub const fn z_source(&self) -> &M2Track<f32> {
        &self.z_source
    }

    /// Returns the RGB ramp sampled across each particle lifetime.
    #[must_use]
    pub const fn color(&self) -> &M2ParticleLifetimeTrack<Vec3> {
        &self.color
    }

    /// Returns the signed-fixed16 opacity ramp expanded to floats.
    #[must_use]
    pub const fn alpha(&self) -> &M2ParticleLifetimeTrack<f32> {
        &self.alpha
    }

    /// Returns the two-axis size ramp sampled across each particle lifetime.
    #[must_use]
    pub const fn scale(&self) -> &M2ParticleLifetimeTrack<Vec2> {
        &self.scale
    }

    /// Returns the authored random X/Y size-variation range.
    #[must_use]
    pub const fn scale_variation(&self) -> Vec2 {
        self.scale_variation
    }

    /// Returns the head flipbook ramp across each particle lifetime.
    #[must_use]
    pub const fn head_uv_animation(&self) -> &M2ParticleLifetimeTrack<u16> {
        &self.head_uv_animation
    }

    /// Returns the tail flipbook ramp across each particle lifetime.
    #[must_use]
    pub const fn tail_uv_animation(&self) -> &M2ParticleLifetimeTrack<u16> {
        &self.tail_uv_animation
    }

    /// Returns tail length in seconds of particle history.
    #[must_use]
    pub const fn tail_length(&self) -> f32 {
        self.tail_length
    }

    /// Returns the authored twinkle rate.
    #[must_use]
    pub const fn twinkle_speed(&self) -> f32 {
        self.twinkle_speed
    }

    /// Returns the fraction of each twinkle period that remains visible.
    #[must_use]
    pub const fn twinkle_percent(&self) -> f32 {
        self.twinkle_percent
    }

    /// Returns the minimum and maximum random twinkle scale.
    #[must_use]
    pub const fn twinkle_scale(&self) -> Vec2 {
        self.twinkle_scale
    }

    /// Returns the multiplier applied to inherited parent velocity.
    #[must_use]
    pub const fn inherit_velocity_scale(&self) -> f32 {
        self.inherit_velocity_scale
    }

    /// Returns exponential velocity drag.
    #[must_use]
    pub const fn drag(&self) -> f32 {
        self.drag
    }

    /// Returns the initial quad rotation.
    #[must_use]
    pub const fn base_spin(&self) -> f32 {
        self.base_spin
    }

    /// Returns signed-random initial rotation variation.
    #[must_use]
    pub const fn base_spin_variation(&self) -> f32 {
        self.base_spin_variation
    }

    /// Returns quad angular velocity.
    #[must_use]
    pub const fn spin_speed(&self) -> f32 {
        self.spin_speed
    }

    /// Returns signed-random angular-velocity variation.
    #[must_use]
    pub const fn spin_speed_variation(&self) -> f32 {
        self.spin_speed_variation
    }

    /// Returns minimum and maximum three-axis tumbling values.
    #[must_use]
    pub const fn tumble(&self) -> (Vec3, Vec3) {
        self.tumble
    }

    /// Returns the authored wind vector.
    #[must_use]
    pub const fn wind_vector(&self) -> Vec3 {
        self.wind_vector
    }

    /// Returns the time scale applied to dynamic wind.
    #[must_use]
    pub const fn wind_time(&self) -> f32 {
        self.wind_time
    }

    /// Returns the two authored follow-position speeds.
    #[must_use]
    pub const fn follow_speed(&self) -> (f32, f32) {
        self.follow_speed
    }

    /// Returns the two authored follow-position scales.
    #[must_use]
    pub const fn follow_scale(&self) -> (f32, f32) {
        self.follow_scale
    }

    /// Returns spline control points for the spline emitter type.
    #[must_use]
    pub fn spline_points(&self) -> &[Vec3] {
        &self.spline_points
    }

    /// Returns the byte-valued emitter enable track.
    #[must_use]
    pub const fn enabled(&self) -> &M2Track<u8> {
        &self.enabled
    }
}

/// Decodes the complete top-level particle table and validates bone ownership.
pub(super) fn decode_particles(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    bone_count: usize,
) -> Result<Vec<M2ParticleEmitter>, AssetError> {
    let array = array_ref(path, bytes, 0x128, "particles")?;
    validate_array(path, bytes, array, 476, "particles")?;
    let mut particles = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 476;
        let field = |name: &str| format!("particle {index} {name}");
        let bone_raw = read_u16(path, bytes, offset + 0x14, &field("bone"))?;
        let bone_index = if bone_raw == u16::MAX {
            None
        } else {
            if usize::from(bone_raw) >= bone_count {
                return Err(model_decode(
                    path,
                    format!("particle {index} references missing bone {bone_raw}"),
                ));
            }
            Some(bone_raw)
        };
        let texture_raw = read_u16(path, bytes, offset + 0x16, &field("texture"))?;
        particles.push(M2ParticleEmitter {
            id: read_u32(path, bytes, offset, &field("ID"))?,
            flags: read_u32(path, bytes, offset + 4, &field("flags"))?,
            position: decode_vec3(path, bytes, offset + 8, &field("position"))?,
            bone_index,
            texture_id: (texture_raw != u16::MAX).then_some(texture_raw),
            geometry_model_path: decode_optional_path(
                path,
                bytes,
                offset + 0x18,
                &field("geometry model"),
            )?,
            child_emitter_model_path: decode_optional_path(
                path,
                bytes,
                offset + 0x20,
                &field("child emitter model"),
            )?,
            blending_type: read_u8(path, bytes, offset + 0x28, &field("blending type"))?,
            emitter_type: read_u8(path, bytes, offset + 0x29, &field("emitter type"))?,
            particle_color_index: read_u16(path, bytes, offset + 0x2a, &field("particle color"))?,
            particle_type: read_u8(path, bytes, offset + 0x2c, &field("particle type"))?,
            head_or_tail: read_u8(path, bytes, offset + 0x2d, &field("head or tail"))?,
            priority_plane: read_i16(path, bytes, offset + 0x2e, &field("priority plane"))?,
            texture_rows: read_u16(path, bytes, offset + 0x30, &field("texture rows"))?,
            texture_columns: read_u16(path, bytes, offset + 0x32, &field("texture columns"))?,
            emission_speed: float_track(
                path,
                bytes,
                offset + 0x034,
                &field("emission speed"),
                globals,
                sequences,
                payloads,
            )?,
            speed_variation: float_track(
                path,
                bytes,
                offset + 0x048,
                &field("speed variation"),
                globals,
                sequences,
                payloads,
            )?,
            vertical_range: float_track(
                path,
                bytes,
                offset + 0x05c,
                &field("vertical range"),
                globals,
                sequences,
                payloads,
            )?,
            horizontal_range: float_track(
                path,
                bytes,
                offset + 0x070,
                &field("horizontal range"),
                globals,
                sequences,
                payloads,
            )?,
            gravity: float_track(
                path,
                bytes,
                offset + 0x084,
                &field("gravity"),
                globals,
                sequences,
                payloads,
            )?,
            lifespan: float_track(
                path,
                bytes,
                offset + 0x098,
                &field("lifespan"),
                globals,
                sequences,
                payloads,
            )?,
            lifespan_variation: read_f32(
                path,
                bytes,
                offset + 0x0ac,
                &field("lifespan variation"),
            )?,
            emission_rate: float_track(
                path,
                bytes,
                offset + 0x0b0,
                &field("emission rate"),
                globals,
                sequences,
                payloads,
            )?,
            emission_rate_variation: read_f32(
                path,
                bytes,
                offset + 0x0c4,
                &field("emission rate variation"),
            )?,
            emission_area_width: float_track(
                path,
                bytes,
                offset + 0x0c8,
                &field("emission area width"),
                globals,
                sequences,
                payloads,
            )?,
            emission_area_length: float_track(
                path,
                bytes,
                offset + 0x0dc,
                &field("emission area length"),
                globals,
                sequences,
                payloads,
            )?,
            z_source: float_track(
                path,
                bytes,
                offset + 0x0f0,
                &field("z source"),
                globals,
                sequences,
                payloads,
            )?,
            color: decode_lifetime_track(
                path,
                bytes,
                offset + 0x104,
                &field("color"),
                12,
                decode_vec3,
            )?,
            alpha: decode_lifetime_track(
                path,
                bytes,
                offset + 0x114,
                &field("alpha"),
                2,
                decode_fixed16,
            )?,
            scale: decode_lifetime_track(
                path,
                bytes,
                offset + 0x124,
                &field("scale"),
                8,
                decode_vec2,
            )?,
            scale_variation: decode_vec2(path, bytes, offset + 0x134, &field("scale variation"))?,
            head_uv_animation: decode_lifetime_track(
                path,
                bytes,
                offset + 0x13c,
                &field("head UV animation"),
                2,
                read_u16,
            )?,
            tail_uv_animation: decode_lifetime_track(
                path,
                bytes,
                offset + 0x14c,
                &field("tail UV animation"),
                2,
                read_u16,
            )?,
            tail_length: read_f32(path, bytes, offset + 0x15c, &field("tail length"))?,
            twinkle_speed: read_f32(path, bytes, offset + 0x160, &field("twinkle speed"))?,
            twinkle_percent: read_f32(path, bytes, offset + 0x164, &field("twinkle percent"))?,
            twinkle_scale: decode_vec2(path, bytes, offset + 0x168, &field("twinkle scale"))?,
            inherit_velocity_scale: read_f32(
                path,
                bytes,
                offset + 0x170,
                &field("inherit velocity scale"),
            )?,
            drag: read_f32(path, bytes, offset + 0x174, &field("drag"))?,
            base_spin: read_f32(path, bytes, offset + 0x178, &field("base spin"))?,
            base_spin_variation: read_f32(
                path,
                bytes,
                offset + 0x17c,
                &field("base spin variation"),
            )?,
            spin_speed: read_f32(path, bytes, offset + 0x180, &field("spin speed"))?,
            spin_speed_variation: read_f32(
                path,
                bytes,
                offset + 0x184,
                &field("spin speed variation"),
            )?,
            tumble: (
                decode_vec3(path, bytes, offset + 0x188, &field("tumble minimum"))?,
                decode_vec3(path, bytes, offset + 0x194, &field("tumble maximum"))?,
            ),
            wind_vector: decode_vec3(path, bytes, offset + 0x1a0, &field("wind vector"))?,
            wind_time: read_f32(path, bytes, offset + 0x1ac, &field("wind time"))?,
            follow_speed: (
                read_f32(path, bytes, offset + 0x1b0, &field("follow speed 1"))?,
                read_f32(path, bytes, offset + 0x1b8, &field("follow speed 2"))?,
            ),
            follow_scale: (
                read_f32(path, bytes, offset + 0x1b4, &field("follow scale 1"))?,
                read_f32(path, bytes, offset + 0x1bc, &field("follow scale 2"))?,
            ),
            spline_points: decode_vec3_array(path, bytes, offset + 0x1c0, &field("spline points"))?,
            enabled: decode_track(
                path,
                bytes,
                offset + 0x1c8,
                &field("enabled"),
                globals,
                sequences,
                payloads,
                1,
                read_u8,
            )?,
        });
    }
    Ok(particles)
}

/// Decodes a conventional float-valued emitter-time animation track.
fn float_track(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<M2Track<f32>, AssetError> {
    decode_track(
        path, bytes, offset, field, globals, sequences, payloads, 4, read_f32,
    )
}

/// Decodes one optional, complete C-string asset path.
fn decode_optional_path(
    model_path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Option<AssetPath>, AssetError> {
    let array = array_ref(model_path, bytes, offset, field)?;
    validate_array(model_path, bytes, array, 1, field)?;
    if array.count == 0 {
        return Ok(None);
    }
    let raw = &bytes[array.offset..array.offset + array.count];
    if raw.last() != Some(&0) || raw[..raw.len() - 1].contains(&0) {
        return Err(model_decode(
            model_path,
            format!("{field} is not one complete C string"),
        ));
    }
    if raw.len() == 1 {
        return Ok(None);
    }
    let value = std::str::from_utf8(&raw[..raw.len() - 1])
        .map_err(|source| model_decode(model_path, format!("{field} is not UTF-8: {source}")))?;
    AssetPath::new(value)
        .map(Some)
        .map_err(|source| model_decode(model_path, format!("{field} is invalid: {source}")))
}

/// Decodes one header-less lifetime ramp with matching timestamp and value arrays.
fn decode_lifetime_track<T>(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
    value_size: usize,
    decode_value: fn(&AssetPath, &[u8], usize, &str) -> Result<T, AssetError>,
) -> Result<M2ParticleLifetimeTrack<T>, AssetError> {
    let timestamps = array_ref(path, bytes, offset, field)?;
    let values = array_ref(path, bytes, offset + 8, field)?;
    validate_array(path, bytes, timestamps, 2, field)?;
    validate_array(path, bytes, values, value_size, field)?;
    if timestamps.count != values.count {
        return Err(model_decode(path, format!("{field} key counts differ")));
    }
    let timestamps = (0..timestamps.count)
        .map(|index| read_u16(path, bytes, timestamps.offset + index * 2, field))
        .collect::<Result<Vec<_>, _>>()?;
    if timestamps
        .iter()
        .any(|timestamp| *timestamp > i16::MAX as u16)
    {
        return Err(model_decode(
            path,
            format!("{field} timestamp exceeds the stock signed fixed16 domain"),
        ));
    }
    if !timestamps.windows(2).all(|pair| pair[0] <= pair[1]) {
        return Err(model_decode(
            path,
            format!("{field} timestamps are not ordered"),
        ));
    }
    let values = (0..values.count)
        .map(|index| decode_value(path, bytes, values.offset + index * value_size, field))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(M2ParticleLifetimeTrack { timestamps, values })
}

/// Decodes one finite two-component vector.
fn decode_vec2(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec2, AssetError> {
    let value = Vec2::new(
        read_f32(path, bytes, offset, field)?,
        read_f32(path, bytes, offset + 4, field)?,
    );
    if !value.is_finite() {
        return Err(model_decode(
            path,
            format!("{field} contains a non-finite vector"),
        ));
    }
    Ok(value)
}

/// Copies spline points without retaining offsets into an HD-sized model body.
fn decode_vec3_array(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec<Vec3>, AssetError> {
    let array = array_ref(path, bytes, offset, field)?;
    validate_array(path, bytes, array, 12, field)?;
    (0..array.count)
        .map(|index| decode_vec3(path, bytes, array.offset + index * 12, field))
        .collect()
}
