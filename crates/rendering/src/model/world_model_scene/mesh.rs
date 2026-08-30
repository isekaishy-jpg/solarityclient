//! Shared root/group WMO geometry combination and stock MOCV fixup.

use solarity_asset::{AssetPath, DecodedWorldModel, WorldModelMaterial};

use super::{
    WorldModelDrawCall, WorldModelGroupRange, WorldModelMeshPlanError, WorldModelRenderVertex,
};

/// Upload-ready shared geometry and materials for one WMO generation.
pub struct WorldModelMeshPlan {
    path: AssetPath,
    vertices: Vec<WorldModelRenderVertex>,
    indices: Vec<u32>,
    materials: Vec<WorldModelMaterial>,
    draws: Vec<WorldModelDrawCall>,
    groups: Vec<WorldModelGroupRange>,
}

impl WorldModelMeshPlan {
    /// Combines independently loaded group files into one immutable mesh.
    ///
    /// The plan retains `u32` combined indices so large HD replacements are
    /// limited by the explicit renderer ABI rather than legacy `u16` totals.
    /// Group-local source indices remain the exact WotLK `u16` values.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelMeshPlanError`] when combined counts exceed `u32`
    /// or a decoded batch cannot be rebased into its group range.
    pub fn prepare(model: &DecodedWorldModel) -> Result<Self, WorldModelMeshPlanError> {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut draws = Vec::new();
        let mut groups = Vec::with_capacity(model.groups().len());
        for group in model.groups() {
            let first_vertex = u32::try_from(vertices.len()).map_err(|_| {
                WorldModelMeshPlanError::IndexCapacity {
                    path: model.path().clone(),
                }
            })?;
            let first_index = u32::try_from(indices.len()).map_err(|_| {
                WorldModelMeshPlanError::IndexCapacity {
                    path: model.path().clone(),
                }
            })?;
            let vertex_count = u32::try_from(group.vertices().len()).map_err(|_| {
                WorldModelMeshPlanError::IndexCapacity {
                    path: model.path().clone(),
                }
            })?;
            let index_count = u32::try_from(group.indices().len()).map_err(|_| {
                WorldModelMeshPlanError::IndexCapacity {
                    path: model.path().clone(),
                }
            })?;
            first_vertex.checked_add(vertex_count).ok_or_else(|| {
                WorldModelMeshPlanError::IndexCapacity {
                    path: model.path().clone(),
                }
            })?;
            let group_index_end = first_index.checked_add(index_count).ok_or_else(|| {
                WorldModelMeshPlanError::IndexCapacity {
                    path: model.path().clone(),
                }
            })?;

            let fixed_colors = fixed_vertex_colors(model.flags(), group);
            for index in 0..group.vertices().len() {
                let primary = group
                    .texture_coordinates()
                    .first()
                    .map_or([0.0; 2], |layer| layer[index]);
                let secondary = group
                    .texture_coordinates()
                    .get(1)
                    .map_or(primary, |layer| layer[index]);
                let blend = group
                    .vertex_colors()
                    .get(1)
                    .map_or([0.0, 0.0, 0.0, 1.0], |layer| bgra(layer[index]));
                vertices.push(WorldModelRenderVertex::new(
                    group.vertices()[index],
                    group.normals()[index],
                    [primary, secondary],
                    bgra(fixed_colors[index]),
                    blend,
                ));
            }
            for index in group.indices() {
                indices.push(first_vertex + u32::from(*index));
            }
            for (batch_index, batch) in group.batches().iter().copied().enumerate() {
                let batch_first =
                    first_index
                        .checked_add(batch.first_index())
                        .ok_or_else(|| WorldModelMeshPlanError::DrawRange {
                            path: model.path().clone(),
                            group_index: group.index(),
                            batch_index,
                        })?;
                let batch_count = u32::from(batch.index_count());
                let batch_end = batch_first.checked_add(batch_count).ok_or_else(|| {
                    WorldModelMeshPlanError::DrawRange {
                        path: model.path().clone(),
                        group_index: group.index(),
                        batch_index,
                    }
                })?;
                if batch_end > group_index_end {
                    return Err(WorldModelMeshPlanError::DrawRange {
                        path: model.path().clone(),
                        group_index: group.index(),
                        batch_index,
                    });
                }
                draws.push(WorldModelDrawCall::new(
                    group.index(),
                    batch_first,
                    batch_count,
                    batch.material_id(),
                    batch.class(),
                    batch.bounds(),
                ));
            }
            groups.push(WorldModelGroupRange::new(
                group.index(),
                first_vertex,
                vertex_count,
                first_index,
                index_count,
                group.flags(),
                group.bounds(),
            ));
        }
        Ok(Self {
            path: model.path().clone(),
            vertices,
            indices,
            materials: model.materials().to_vec(),
            draws,
            groups,
        })
    }

    /// Returns the canonical root-WMO resource identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns combined fixed-layout vertices.
    #[must_use]
    pub fn vertices(&self) -> &[WorldModelRenderVertex] {
        &self.vertices
    }

    /// Returns combined direct `u32` indices.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Returns root MOMT materials in exact table order.
    #[must_use]
    pub fn materials(&self) -> &[WorldModelMaterial] {
        &self.materials
    }

    /// Returns every rebased MOBA draw in group/file order.
    #[must_use]
    pub fn draws(&self) -> &[WorldModelDrawCall] {
        &self.draws
    }

    /// Returns every group's immutable combined-buffer domain.
    #[must_use]
    pub fn groups(&self) -> &[WorldModelGroupRange] {
        &self.groups
    }

    /// Serializes vertices without depending on Rust layout.
    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * WorldModelRenderVertex::BYTE_SIZE);
        for vertex in &self.vertices {
            vertex.append_bytes(&mut bytes);
        }
        bytes
    }

    /// Serializes direct indices without depending on host endianness.
    #[must_use]
    pub fn index_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.indices.len() * size_of::<u32>());
        for index in &self.indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes
    }
}

fn fixed_vertex_colors(
    root_flags: u16,
    group: &solarity_asset::DecodedWorldModelGroup,
) -> Vec<[u8; 4]> {
    let mut colors = group
        .vertex_colors()
        .first()
        .cloned()
        .unwrap_or_else(|| vec![[255; 4]; group.vertices().len()]);
    // CWmoGroup::Load at 0x007D7CE6 invokes FixColorVertexAlpha at
    // 0x007D7380 unless MOHD bit 0x8 requests the already-authored values.
    if root_flags & 0x8 != 0 {
        return colors;
    }
    let transition_count = usize::from(group.batch_counts()[0]);
    let interior_start = if transition_count != 0 && transition_count <= group.batches().len() {
        usize::from(group.batches()[transition_count - 1].vertex_range()[1]) + 1
    } else {
        0
    }
    .min(colors.len());
    for (index, color) in colors.iter_mut().enumerate() {
        if index < interior_start {
            color[0] >>= 1;
            color[1] >>= 1;
            color[2] >>= 1;
            continue;
        }
        let alpha = u32::from(color[3]);
        for channel in &mut color[..3] {
            let source = u32::from(*channel);
            // The explicit clamp proves the narrowing conversion is lossless.
            *channel = ((source + ((source * alpha) >> 6)) >> 1).min(255) as u8;
        }
        color[3] = 255;
    }
    colors
}

const fn bgra(value: [u8; 4]) -> [f32; 4] {
    const SCALE: f32 = 1.0 / 255.0;
    [
        value[2] as f32 * SCALE,
        value[1] as f32 * SCALE,
        value[0] as f32 * SCALE,
        value[3] as f32 * SCALE,
    ]
}
