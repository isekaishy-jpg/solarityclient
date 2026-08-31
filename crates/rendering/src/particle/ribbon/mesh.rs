//! Upload-ready PCT0 triangle-strip preparation for one live ribbon.

use solarity_asset::M2RibbonEmitter;
use thiserror::Error;

use super::M2RibbonTrail;
use crate::particle::pack_bgra;

/// Fixed 24-byte ribbon vertex matching stock's `CGxVertexPCT0` payload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2RibbonRenderVertex {
    position: [f32; 3],
    color_bgra: [u8; 4],
    texture_coordinates: [f32; 2],
}

impl M2RibbonRenderVertex {
    /// Size of one explicitly serialized ribbon vertex.
    pub const BYTE_SIZE: usize = 24;

    /// Returns the already transformed world-space strip position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns stock's packed color in little-endian BGRA byte order.
    #[must_use]
    pub const fn color_bgra(self) -> [u8; 4] {
        self.color_bgra
    }

    /// Returns the age-progressed coordinates inside the selected atlas cell.
    #[must_use]
    pub const fn texture_coordinates(self) -> [f32; 2] {
        self.texture_coordinates
    }

    pub(crate) fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        for component in self.position {
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
        let mut vertices = Vec::new();
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
            vertices.push(M2RibbonRenderVertex {
                position: section.above().to_array(),
                color_bgra,
                texture_coordinates: [u, cell_v],
            });
            vertices.push(M2RibbonRenderVertex {
                position: section.below().to_array(),
                color_bgra,
                texture_coordinates: [u, next_v],
            });
        }
        Ok(Self { vertices })
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
