//! Upload-ready ordinary particle head and tail preparation.

use std::borrow::Cow;

use glam::{Quat, Vec2, Vec3, Vec4};
use solarity_asset::M2ParticleEmitter;
use thiserror::Error;

use crate::WorldCameraFrame;

use super::{
    M2ParticleColorReplacement, M2ParticleLifetimePose, M2ParticleLifetimePoseError,
    M2ParticlePose, M2ParticleRotationPose, M2ParticleState, M2ParticleTwinkleError,
    M2ParticleTwinkleTable,
};
use crate::particle::pack_bgra;

/// Recovered branches shared by the ordinary head and tail preparation.
const UNSUPPORTED_SHARED_FLAGS: u32 = 0x0100_0000;

/// Sorts live cards back-to-front inside one compatible emitter batch.
const SORT_PARTICLES: u32 = 0x0000_0002;

/// Limits a tail's history span to the particle's current age.
const CLAMP_TAIL_TO_AGE: u32 = 0x0000_0400;

/// Emits the ordinary camera-facing head geometry.
const HEAD_STYLE: u32 = 0x0002_0000;

/// Emits the velocity-history tail geometry.
const TAIL_STYLE: u32 = 0x0004_0000;

/// Keeps head offsets in the transformed emitter X/Y basis.
const FIXED_EMITTER_BASIS_HEAD: u32 = 0x0000_4000;

/// Alternates the authored head spin direction between adjacent pool slots.
const ALTERNATING_HEAD_ROTATION: u32 = 0x0001_0000;

/// Aligns the head to its camera-projected velocity with foreshortening.
const VELOCITY_ALIGNED_HEAD: u32 = 0x0020_0000;

/// Exact direction threshold loaded at executable address `0x009EA27C`.
const DIRECTION_THRESHOLD_SQUARED: f32 = f32::from_bits(0x3480_0000);

/// Exact projected tail-length threshold loaded at executable `0x00AA2CEC`.
const TAIL_PROJECTION_THRESHOLD_SQUARED: f32 = f32::from_bits(0x3a4a_4588);

/// Executable corner table at `0x00B2D5B4` in triangle-strip order.
const CORNERS: [Vec2; 4] = [
    Vec2::new(-1.0, 1.0),
    Vec2::new(-1.0, -1.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(1.0, -1.0),
];

/// Executable atlas-coordinate table at `0x00B2D5D4`.
const CELL_COORDINATES: [Vec2; 4] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(1.0, 1.0),
];

/// Fixed 36-byte particle vertex matching stock's PNC0T0 streams.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ParticleRenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color_bgra: [u8; 4],
    texture_coordinates: [f32; 2],
}

impl M2ParticleRenderVertex {
    /// Size of one explicitly serialized particle vertex.
    pub const BYTE_SIZE: usize = 36;

    /// Returns the camera-facing world-space position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the world-space normal facing the camera.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns stock's packed color in little-endian BGRA byte order.
    #[must_use]
    pub const fn color_bgra(self) -> [u8; 4] {
        self.color_bgra
    }

    /// Returns coordinates inside the particle's held atlas cell.
    #[must_use]
    pub const fn texture_coordinates(self) -> [f32; 2] {
        self.texture_coordinates
    }

    pub(crate) fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        for component in self.position.into_iter().chain(self.normal) {
            bytes[offset..offset + 4].copy_from_slice(&component.to_le_bytes());
            offset += 4;
        }
        bytes[offset..offset + 4].copy_from_slice(&self.color_bgra);
        offset += 4;
        for component in self.texture_coordinates {
            bytes[offset..offset + 4].copy_from_slice(&component.to_le_bytes());
            offset += 4;
        }
        bytes
    }
}

/// Dynamic ordinary-particle vertices and triangle indices for one emitter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct M2ParticleMeshPlan {
    vertices: Vec<M2ParticleRenderVertex>,
    indices: Vec<u32>,
}

impl M2ParticleMeshPlan {
    /// Builds ordinary head and tail quads from one placement-local live set.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleMeshPlanError`] when authored atlas or behavior
    /// state selects a different stock path, or dynamic storage overflows.
    pub fn prepare(
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        particles: &[M2ParticleState],
        camera: WorldCameraFrame,
        alpha_multiplier: f32,
    ) -> Result<Self, M2ParticleMeshPlanError> {
        Self::prepare_internal(
            emitter,
            pose,
            particles,
            camera,
            glam::Mat4::IDENTITY,
            alpha_multiplier,
            None,
            None,
        )
    }

    /// Builds ordinary geometry with the process-wide stock twinkle phases.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::prepare`].
    pub fn prepare_with_twinkle_table(
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        particles: &[M2ParticleState],
        camera: WorldCameraFrame,
        alpha_multiplier: f32,
        twinkle_table: &M2ParticleTwinkleTable,
    ) -> Result<Self, M2ParticleMeshPlanError> {
        Self::prepare_internal(
            emitter,
            pose,
            particles,
            camera,
            glam::Mat4::IDENTITY,
            alpha_multiplier,
            Some(twinkle_table),
            None,
        )
    }

    /// Builds ordinary geometry while resolving simulation space to world.
    ///
    /// World-space simulations pass identity. Flag-`0x200` simulations pass
    /// the current placement/bone/emitter matrix so particles follow it.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::prepare`], plus an invalid transform.
    pub fn prepare_transformed(
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        particles: &[M2ParticleState],
        camera: WorldCameraFrame,
        particle_to_world: glam::Mat4,
        alpha_multiplier: f32,
    ) -> Result<Self, M2ParticleMeshPlanError> {
        Self::prepare_internal(
            emitter,
            pose,
            particles,
            camera,
            particle_to_world,
            alpha_multiplier,
            None,
            None,
        )
    }

    /// Builds transformed ordinary geometry with process-wide twinkle phases.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::prepare_transformed`].
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_transformed_with_twinkle_table(
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        particles: &[M2ParticleState],
        camera: WorldCameraFrame,
        particle_to_world: glam::Mat4,
        alpha_multiplier: f32,
        twinkle_table: &M2ParticleTwinkleTable,
    ) -> Result<Self, M2ParticleMeshPlanError> {
        Self::prepare_internal(
            emitter,
            pose,
            particles,
            camera,
            particle_to_world,
            alpha_multiplier,
            Some(twinkle_table),
            None,
        )
    }

    /// Builds transformed geometry with twinkle and placement-local colors.
    ///
    /// Static world placements pass no replacement through the existing entry
    /// points. Creature and item owners use this boundary after their display
    /// metadata has selected one `ParticleColor.dbc` row.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::prepare_transformed_with_twinkle_table`].
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_transformed_with_particle_color(
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        particles: &[M2ParticleState],
        camera: WorldCameraFrame,
        particle_to_world: glam::Mat4,
        alpha_multiplier: f32,
        twinkle_table: &M2ParticleTwinkleTable,
        replacement: Option<&M2ParticleColorReplacement>,
    ) -> Result<Self, M2ParticleMeshPlanError> {
        Self::prepare_internal(
            emitter,
            pose,
            particles,
            camera,
            particle_to_world,
            alpha_multiplier,
            Some(twinkle_table),
            replacement,
        )
    }

    /// Shared ordinary preparation with an optional explicit twinkle owner.
    #[allow(clippy::too_many_arguments)]
    fn prepare_internal(
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        particles: &[M2ParticleState],
        camera: WorldCameraFrame,
        particle_to_world: glam::Mat4,
        alpha_multiplier: f32,
        twinkle_table: Option<&M2ParticleTwinkleTable>,
        replacement: Option<&M2ParticleColorReplacement>,
    ) -> Result<Self, M2ParticleMeshPlanError> {
        if !alpha_multiplier.is_finite() {
            return Err(M2ParticleMeshPlanError::AlphaMultiplier);
        }
        let determinant = particle_to_world.determinant();
        if !particle_to_world.is_finite()
            || !determinant.is_finite()
            || determinant.abs() <= f32::EPSILON
        {
            return Err(M2ParticleMeshPlanError::Transform);
        }
        let inverse_particle_transform = particle_to_world.inverse();
        let billboard_right = particle_to_world.transform_vector3(
            inverse_particle_transform
                .transform_vector3(camera.right())
                .normalize(),
        );
        let billboard_up = particle_to_world.transform_vector3(
            inverse_particle_transform
                .transform_vector3(camera.up())
                .normalize(),
        );
        let emitter_right = particle_to_world.transform_vector3(Vec3::X);
        let emitter_up = particle_to_world.transform_vector3(Vec3::Y);
        let emitter_normal = particle_to_world.transform_vector3(Vec3::Z).normalize();
        let style_flags = emitter.flags() & (HEAD_STYLE | TAIL_STYLE);
        let (emit_head, emit_tail) = if style_flags != 0 {
            (style_flags & HEAD_STYLE != 0, style_flags & TAIL_STYLE != 0)
        } else {
            // Build 12340 authors stock styles in the flags. Retain the legacy
            // byte for custom assets which set neither style flag.
            match emitter.head_or_tail() {
                0 => (true, false),
                1 => (false, true),
                2 => (true, true),
                selector => return Err(M2ParticleMeshPlanError::HeadOrTail(selector)),
            }
        };
        if emitter.texture_rows() == 0 || emitter.texture_columns() == 0 {
            return Err(M2ParticleMeshPlanError::EmptyTextureAtlas);
        }
        if !emitter.texture_columns().is_power_of_two() {
            return Err(M2ParticleMeshPlanError::AtlasColumns);
        }
        if emitter.geometry_model_path().is_some() {
            return Err(M2ParticleMeshPlanError::GeometryParticle);
        }
        let unsupported = emitter.flags() & UNSUPPORTED_SHARED_FLAGS;
        if unsupported != 0 {
            return Err(M2ParticleMeshPlanError::BehaviorFlags(unsupported));
        }
        let twinkle_active = emitter.twinkle_percent() < 1.0 || emitter.twinkle_scale().y != 0.0;
        if twinkle_active && twinkle_table.is_none() {
            return Err(M2ParticleMeshPlanError::TwinkleTable);
        }
        if emit_tail && !emitter.tail_length().is_finite() {
            return Err(M2ParticleMeshPlanError::TailLength);
        }

        let quads_per_particle = usize::from(emit_head) + usize::from(emit_tail);
        let quad_count = particles
            .len()
            .checked_mul(quads_per_particle)
            .ok_or(M2ParticleMeshPlanError::VertexCount)?;
        let vertex_count = particles
            .len()
            .checked_mul(quads_per_particle)
            .and_then(|count| count.checked_mul(4))
            .ok_or(M2ParticleMeshPlanError::VertexCount)?;
        let index_count = quad_count
            .checked_mul(6)
            .ok_or(M2ParticleMeshPlanError::IndexCount)?;
        u32::try_from(vertex_count).map_err(|_source| M2ParticleMeshPlanError::VertexCount)?;
        let mut vertices = Vec::new();
        vertices
            .try_reserve_exact(vertex_count)
            .map_err(|_source| M2ParticleMeshPlanError::VertexCount)?;
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(index_count)
            .map_err(|_source| M2ParticleMeshPlanError::IndexCount)?;

        let columns = u32::from(emitter.texture_columns());
        let rows = f32::from(emitter.texture_rows());
        let cell_size = Vec2::new(1.0 / columns as f32, rows.recip());
        let normal = -camera.forward();
        let ordered_particles = if emitter.flags() & SORT_PARTICLES != 0 {
            // CM2Model's recovered render path sorts a temporary presentation
            // list; simulation slots and their address-derived twinkle phases
            // must remain untouched.
            let mut sorted = particles.to_vec();
            sorted.sort_by(|left, right| {
                let distance = |particle: &M2ParticleState| {
                    particle_to_world
                        .transform_point3(particle.position())
                        .distance_squared(camera.camera().position())
                };
                distance(right)
                    .partial_cmp(&distance(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            Cow::Owned(sorted)
        } else {
            Cow::Borrowed(particles)
        };
        for particle in ordered_particles.iter() {
            let twinkle_scale = match twinkle_table {
                Some(table) => match table.sample(emitter, particle)? {
                    Some(scale) => scale,
                    None => continue,
                },
                None => 1.0,
            };
            let position = particle_to_world.transform_point3(particle.position());
            let velocity = particle_to_world.transform_vector3(particle.velocity());
            let appearance = M2ParticleLifetimePose::sample_with_particle_color(
                emitter,
                particle.normalized_age(pose.lifespan(), emitter.lifespan_variation()),
                particle.random_word(),
                replacement,
            )?;
            let mut color = appearance.color();
            color.w *= alpha_multiplier;
            if emit_head {
                let scale = appearance.scale() * twinkle_scale;
                let positions = if emitter.flags() & VELOCITY_ALIGNED_HEAD != 0
                    && particle.velocity().length_squared() > DIRECTION_THRESHOLD_SQUARED
                {
                    velocity_aligned_billboard_positions(
                        position,
                        scale,
                        velocity,
                        camera,
                        billboard_right,
                        billboard_up,
                    )
                } else {
                    let mut rotation =
                        M2ParticleRotationPose::sample(emitter, particle.random_word())
                            .angle_radians(particle.age_seconds());
                    if emitter.flags() & ALTERNATING_HEAD_ROTATION != 0
                        && std::ptr::from_ref(particle).addr() & 0x20 != 0
                    {
                        rotation = -rotation;
                    }
                    if emitter.flags() & FIXED_EMITTER_BASIS_HEAD != 0 {
                        fixed_basis_positions(
                            position,
                            scale,
                            rotation,
                            emitter_right,
                            emitter_up,
                            emitter_normal,
                        )
                    } else {
                        billboard_positions(
                            position,
                            scale,
                            rotation,
                            billboard_right,
                            billboard_up,
                        )
                    }
                };
                push_quad(
                    &mut vertices,
                    &mut indices,
                    positions,
                    normal,
                    color,
                    appearance.head_texture_cell(),
                    columns,
                    cell_size,
                )?;
            }
            if emit_tail {
                let mut span = emitter.tail_length();
                if emitter.flags() & CLAMP_TAIL_TO_AGE != 0 && particle.age_seconds() < span {
                    span = particle.age_seconds();
                }
                let tail_vector = -velocity * span;
                let projected = Vec2::new(
                    tail_vector.dot(camera.right()),
                    tail_vector.dot(camera.up()),
                );
                let positions = if projected.length_squared() > TAIL_PROJECTION_THRESHOLD_SQUARED {
                    let reciprocal_length = projected.length_recip();
                    let side = -billboard_right
                        * (appearance.scale().y * twinkle_scale * projected.y * reciprocal_length)
                        + billboard_up
                            * (appearance.scale().x
                                * twinkle_scale
                                * projected.x
                                * reciprocal_length);
                    let endpoint = position + tail_vector;
                    [
                        position + side,
                        position - side,
                        endpoint + side,
                        endpoint - side,
                    ]
                } else {
                    billboard_positions(
                        position,
                        appearance.scale() * twinkle_scale,
                        0.0,
                        billboard_right,
                        billboard_up,
                    )
                };
                push_quad(
                    &mut vertices,
                    &mut indices,
                    positions,
                    normal,
                    color,
                    appearance.tail_texture_cell(),
                    columns,
                    cell_size,
                )?;
            }
        }
        Ok(Self { vertices, indices })
    }

    /// Returns ordinary billboard vertices in live-particle storage order.
    #[must_use]
    pub fn vertices(&self) -> &[M2ParticleRenderVertex] {
        &self.vertices
    }

    /// Returns the two triangles emitted for every selected ordinary quad.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Serializes PNC0T0 vertices without relying on Rust memory layout.
    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * M2ParticleRenderVertex::BYTE_SIZE);
        for vertex in &self.vertices {
            bytes.extend_from_slice(&vertex.to_bytes());
        }
        bytes
    }
}

/// Produces the executable corner table in the current camera basis.
fn billboard_positions(
    center: Vec3,
    scale: Vec2,
    rotation: f32,
    right: Vec3,
    up: Vec3,
) -> [Vec3; 4] {
    let (sine, cosine) = rotation.sin_cos();
    CORNERS.map(|corner| {
        let scaled = corner * scale;
        let rotated = Vec2::new(
            scaled.x * cosine - scaled.y * sine,
            scaled.x * sine + scaled.y * cosine,
        );
        center + right * rotated.x + up * rotated.y
    })
}

/// Applies local X/Y offsets before rotating around transformed local Z.
fn fixed_basis_positions(
    center: Vec3,
    scale: Vec2,
    rotation: f32,
    right: Vec3,
    up: Vec3,
    normal: Vec3,
) -> [Vec3; 4] {
    let rotation = Quat::from_axis_angle(normal, rotation);
    CORNERS.map(|corner| {
        let offset = right * (corner.x * scale.x) + up * (corner.y * scale.y);
        center + rotation * offset
    })
}

/// Reproduces the camera-projected, foreshortened velocity head basis.
fn velocity_aligned_billboard_positions(
    center: Vec3,
    scale: Vec2,
    velocity: Vec3,
    camera: WorldCameraFrame,
    right: Vec3,
    up: Vec3,
) -> [Vec3; 4] {
    let direction = -velocity;
    let projected = Vec2::new(direction.dot(camera.right()), direction.dot(camera.up()));
    let projected_length_squared = projected.length_squared();
    let reciprocal_projected_length = if projected_length_squared <= DIRECTION_THRESHOLD_SQUARED {
        0.0
    } else {
        projected_length_squared.sqrt().recip()
    };
    let aligned_scale = if reciprocal_projected_length > DIRECTION_THRESHOLD_SQUARED {
        scale.x * direction.length_recip() / reciprocal_projected_length
    } else {
        scale.x
    };
    let along = projected * reciprocal_projected_length;
    CORNERS.map(|corner| {
        let offset = Vec2::new(
            corner.x * aligned_scale * along.x - corner.y * scale.y * along.y,
            corner.y * scale.y * along.x + corner.x * aligned_scale * along.y,
        );
        center + right * offset.x + up * offset.y
    })
}

/// Appends one stock-ordered quad without relying on Rust struct layout.
#[allow(clippy::too_many_arguments)]
fn push_quad(
    vertices: &mut Vec<M2ParticleRenderVertex>,
    indices: &mut Vec<u32>,
    positions: [Vec3; 4],
    normal: Vec3,
    color: Vec4,
    cell: u32,
    columns: u32,
    cell_size: Vec2,
) -> Result<(), M2ParticleMeshPlanError> {
    let cell_origin = Vec2::new((cell & (columns - 1)) as f32, (cell / columns) as f32) * cell_size;
    let base_vertex =
        u32::try_from(vertices.len()).map_err(|_source| M2ParticleMeshPlanError::VertexCount)?;
    let color_bgra = pack_bgra(color.to_array());
    for (position, coordinate) in positions.into_iter().zip(CELL_COORDINATES) {
        vertices.push(M2ParticleRenderVertex {
            position: position.to_array(),
            normal: normal.to_array(),
            color_bgra,
            texture_coordinates: (cell_origin + coordinate * cell_size).to_array(),
        });
    }
    indices.extend_from_slice(&[
        base_vertex,
        base_vertex + 1,
        base_vertex + 2,
        base_vertex + 2,
        base_vertex + 1,
        base_vertex + 3,
    ]);
    Ok(())
}

/// One emitter cannot enter the ordinary particle mesh path.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2ParticleMeshPlanError {
    /// Stock atlas addressing divides by both authored dimensions.
    #[error("M2 particle texture atlas dimensions must be nonzero")]
    EmptyTextureAtlas,
    /// Stock stores a base-two column shift for head-cell addressing.
    #[error("M2 particle texture atlas columns must be a power of two")]
    AtlasColumns,
    /// The authored selector is not head, tail, or both.
    #[error("M2 particle head-or-tail selector {0} is invalid")]
    HeadOrTail(u8),
    /// Geometry particles use their model-owned vertex path.
    #[error("M2 geometry particle cannot use the ordinary particle mesh")]
    GeometryParticle,
    /// One or more flags select a specialized recovered render path.
    #[error("M2 particle behavior flags 0x{0:08X} are not implemented")]
    BehaviorFlags(u32),
    /// Active twinkle requires the process-wide phase table initialized once.
    #[error("M2 particle twinkle requires the process-wide phase table")]
    TwinkleTable,
    /// A selected tail cannot produce finite dynamic vertices.
    #[error("M2 particle tail length must be finite")]
    TailLength,
    /// Placement/model opacity must remain finite before color packing.
    #[error("M2 particle alpha multiplier must be finite")]
    AlphaMultiplier,
    /// Model-space particles require one finite current emitter transform.
    #[error("M2 particle transform must be finite")]
    Transform,
    /// Four vertices per live particle exceed process or GPU index limits.
    #[error("M2 particle vertex count exceeds process limits")]
    VertexCount,
    /// Six indices per live particle exceed process limits.
    #[error("M2 particle index count exceeds process limits")]
    IndexCount,
    /// One particle's normalized lifetime sample is invalid.
    #[error(transparent)]
    Lifetime(#[from] M2ParticleLifetimePoseError),
    /// One particle cannot enter the signed twinkle phase calculation.
    #[error(transparent)]
    Twinkle(#[from] M2ParticleTwinkleError),
}
