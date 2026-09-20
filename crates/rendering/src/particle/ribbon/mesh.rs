//! Upload-ready PCT0 triangle-strip preparation for one live ribbon.

use solarity_asset::M2RibbonEmitter;
use thiserror::Error;

use super::M2RibbonTrail;
use crate::particle::pack_bgra;

use super::M2RibbonRenderVertex;

/// Failure while translating a ribbon history into a GPU strip.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2RibbonMeshPlanError {
    /// Stock atlas division requires both authored dimensions to be nonzero.
    #[error("M2 ribbon texture atlas dimensions must be nonzero")]
    EmptyTextureAtlas,
    /// The two vertices per retained section exceed process-sized storage.
    #[error("M2 ribbon vertex count exceeds process limits")]
    VertexCount,
}

/// Dynamic strip vertices for one placement-local ribbon emitter.
#[derive(Clone, Debug, PartialEq)]
pub struct M2RibbonMeshPlan {
    vertices: Vec<M2RibbonRenderVertex>,
}

impl M2RibbonMeshPlan {
    /// Converts retained edge pairs to stock's oldest-to-live strip order.
    ///
    /// # Errors
    ///
    /// Returns [`M2RibbonMeshPlanError`] for a zero-sized atlas or if the
    /// required dynamic vertex allocation cannot be represented or reserved.
    pub fn prepare(
        emitter: &M2RibbonEmitter,
        trail: &M2RibbonTrail,
    ) -> Result<Self, M2RibbonMeshPlanError> {
        let mut vertices = Vec::new();
        Self::append(emitter, trail, &mut vertices)?;
        Ok(Self { vertices })
    }

    /// Appends one ribbon strip directly to retained frame storage.
    ///
    /// The destination is restored to its original length if preparation
    /// fails, so callers never retain a partial strip.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::prepare`].
    pub fn append(
        emitter: &M2RibbonEmitter,
        trail: &M2RibbonTrail,
        vertices: &mut impl solarity_cpu::OutputBuffer<M2RibbonRenderVertex>,
    ) -> Result<usize, M2RibbonMeshPlanError> {
        let first_vertex = vertices.len();
        match Self::append_internal(emitter, trail, vertices) {
            Ok(vertex_count) => Ok(vertex_count),
            Err(error) => {
                vertices.truncate(first_vertex);
                Err(error)
            }
        }
    }

    fn append_internal(
        emitter: &M2RibbonEmitter,
        trail: &M2RibbonTrail,
        vertices: &mut impl solarity_cpu::OutputBuffer<M2RibbonRenderVertex>,
    ) -> Result<usize, M2RibbonMeshPlanError> {
        let rows = emitter.texture_rows();
        let columns = emitter.texture_columns();
        if rows == 0 || columns == 0 {
            return Err(M2RibbonMeshPlanError::EmptyTextureAtlas);
        }
        let vertex_count = trail
            .sections()
            .len()
            .checked_mul(2)
            .ok_or(M2RibbonMeshPlanError::VertexCount)?;
        vertices
            .try_reserve_exact(vertex_count)
            .map_err(|_source| M2RibbonMeshPlanError::VertexCount)?;

        let slot = u32::from(trail.texture_slot());
        let columns = f32::from(columns);
        let rows = f32::from(rows);
        let cell_u = (slot % u32::from(emitter.texture_columns())) as f32 / columns;
        let cell_v = (slot / u32::from(emitter.texture_columns())) as f32 / rows;
        let next_v = cell_v + rows.recip();
        for section in trail.sections() {
            let u = cell_u + section.age_seconds() / trail.edge_lifetime_seconds() / columns;
            let color_bgra = pack_bgra(section.color().to_array());
            vertices
                .push(M2RibbonRenderVertex {
                    position: section.above().to_array(),
                    color_bgra,
                    texture_coordinates: [u, cell_v],
                })
                .map_err(|_| M2RibbonMeshPlanError::VertexCount)?;
            vertices
                .push(M2RibbonRenderVertex {
                    position: section.below().to_array(),
                    color_bgra,
                    texture_coordinates: [u, next_v],
                })
                .map_err(|_| M2RibbonMeshPlanError::VertexCount)?;
        }
        Ok(vertex_count)
    }

    /// Returns edge pairs ordered from the oldest retained section to live.
    #[must_use]
    pub fn vertices(&self) -> &[M2RibbonRenderVertex] {
        &self.vertices
    }

    /// Serializes PCT0 vertices without relying on Rust layout or unsafe casts.
    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * M2RibbonRenderVertex::BYTE_SIZE);
        for vertex in &self.vertices {
            bytes.extend_from_slice(&vertex.to_bytes());
        }
        bytes
    }
}
