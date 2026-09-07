//! Typed liquid factory inputs and the prepared WMO mesh boundary.

use solarity_asset::DecodedWorldModel;
use thiserror::Error;

use super::geometry;
use crate::liquid::{LiquidDepthCoordinates, LiquidRenderVertex};

/// Vertex inputs selected by the WMO liquid material factory at 793D20.
#[derive(Clone, Copy, Debug)]
pub enum WorldModelLiquidSurface {
    /// Position-derived surface UVs and an authored depth-byte lookup.
    Water {
        /// Depth bank of the original group type, before the interior material remap.
        depth: Option<LiquidDepthCoordinates>,
        /// Horizontal depth lookup: zero outdoors and one for interior water.
        depth_column: WorldModelLiquidDepthColumn,
        /// MOMT diffuse color indoors, opaque white outdoors, in RGBA order.
        color: [u8; 4],
    },
    /// Signed fixed-point MLIQ UVs; the native magma format has no depth UVs.
    Magma {
        /// Material factory tint in RGBA order.
        color: [u8; 4],
    },
}

/// WMO depth texture column selected by native 7D4370.
#[derive(Clone, Copy, Debug)]
pub enum WorldModelLiquidDepthColumn {
    /// Shared outdoor depth column.
    Exterior,
    /// WMO-specific interior depth column.
    Interior,
}

/// A decoded WMO group exceeds the render mesh's addressable domain.
#[derive(Debug, Error)]
pub enum WorldModelLiquidMeshError {
    /// The requested group does not belong to the decoded model.
    #[error("world model liquid group {0} is out of range")]
    GroupIndex(usize),
    /// The native triangle strip uses sixteen-bit vertex indices.
    #[error("world model liquid mesh exceeds sixteen-bit vertex indices")]
    VertexCapacity,
}

/// One group-local MLIQ mesh, including authored portal-clipped cells.
pub struct WorldModelLiquidMeshPlan {
    vertices: Box<[LiquidRenderVertex]>,
    indices: Box<[u16]>,
}

impl WorldModelLiquidMeshPlan {
    /// Prepares native 7A7CC0/7A7920/7A7F60 output with an identity local matrix.
    ///
    /// The caller applies the WMO instance transform at draw time. Groups without
    /// MLIQ return `None`; absent cells retain their regular grid vertices.
    ///
    /// # Errors
    /// Returns an error for an invalid group index or an unaddressable mesh.
    pub fn prepare(
        model: &DecodedWorldModel,
        group_index: usize,
        surface: WorldModelLiquidSurface,
    ) -> Result<Option<Self>, WorldModelLiquidMeshError> {
        let group = model
            .groups()
            .get(group_index)
            .ok_or(WorldModelLiquidMeshError::GroupIndex(group_index))?;
        let Some(liquid) = group.liquid() else {
            return Ok(None);
        };
        let (vertices, indices) = geometry::prepare(model, group, liquid, surface)?;
        Ok(Some(Self {
            vertices: vertices.into_boxed_slice(),
            indices: indices.into_boxed_slice(),
        }))
    }

    /// Returns regular row-major vertices followed by clipped cell vertices.
    #[must_use]
    pub fn vertices(&self) -> &[LiquidRenderVertex] {
        &self.vertices
    }

    /// Returns the native triangle strip, including restart degenerates.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }
}
