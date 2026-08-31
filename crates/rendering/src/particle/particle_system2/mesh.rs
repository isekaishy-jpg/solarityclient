//! Upload-ready ordinary particle head and tail preparation.

use glam::{Vec2, Vec3, Vec4};
use solarity_asset::M2ParticleEmitter;
use thiserror::Error;

use crate::WorldCameraFrame;

use super::{
    M2ParticleLifetimePose, M2ParticleLifetimePoseError, M2ParticlePose, M2ParticleRotationPose,
    M2ParticleState,
};
use crate::particle::pack_bgra;

/// Recovered branches shared by the ordinary head and tail preparation.
const UNSUPPORTED_SHARED_FLAGS: u32 = 0x0000_0400 | 0x0100_0000;

/// Recovered branches that replace the ordinary camera-facing head quad.
const UNSUPPORTED_HEAD_FLAGS: u32 = 0x0000_4000 | 0x0001_0000 | 0x0020_0000;

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
        Self::prepare_transformed(
            emitter,
            pose,
            particles,
            camera,
            glam::Mat4::IDENTITY,
            alpha_multiplier,
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
        let (emit_head, emit_tail) = match emitter.head_or_tail() {
            0 => (true, false),
            1 => (false, true),
            2 => (true, true),
            selector => return Err(M2ParticleMeshPlanError::HeadOrTail(selector)),
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
        let selected_flags =
            UNSUPPORTED_SHARED_FLAGS | if emit_head { UNSUPPORTED_HEAD_FLAGS } else { 0 };
        let unsupported = emitter.flags() & selected_flags;
        if unsupported != 0 {
            return Err(M2ParticleMeshPlanError::BehaviorFlags(unsupported));
        }
        if emitter.twinkle_percent() < 1.0 || emitter.twinkle_scale().y != 0.0 {
            return Err(M2ParticleMeshPlanError::Twinkle);
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
        for particle in particles {
            let position = particle_to_world.transform_point3(particle.position());
            let velocity = particle_to_world.transform_vector3(particle.velocity());
            let appearance = M2ParticleLifetimePose::sample(
                emitter,
                particle.normalized_age(pose.lifespan(), emitter.lifespan_variation()),
                particle.random_word(),
            )?;
            let mut color = appearance.color();
            color.w *= alpha_multiplier;
            if emit_head {
                let rotation = M2ParticleRotationPose::sample(emitter, particle.random_word())
                    .angle_radians(particle.age_seconds());
                let positions = billboard_positions(
                    position,
                    appearance.scale(),
                    rotation,
                    billboard_right,
                    billboard_up,
                );
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
                if emitter.flags() & 0x0002_0000 != 0 && particle.age_seconds() < span {
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
                        * (appearance.scale().y * projected.y * reciprocal_length)
                        + billboard_up * (appearance.scale().x * projected.x * reciprocal_length);
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
                        appearance.scale(),
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
    /// Stock's pointer-phased twinkle path requires separate identity state.
    #[error("M2 particle twinkle requires the stock phased render path")]
    Twinkle,
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
}
