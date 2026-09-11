//! Shared root/group WMO geometry combination and stock MOCV fixup.

use solarity_asset::{AssetPath, DecodedWorldModel, WorldModelBlendMode, WorldModelMaterial};

use super::{
    WorldModelDrawCall, WorldModelGroupRange, WorldModelMeshPlanError, WorldModelRenderVertex,
};

/// Upload-ready shared geometry and materials for one WMO generation.
pub struct WorldModelMeshPlan {
    path: AssetPath,
    root_flags: u16,
    ambient_color: [u8; 4],
    vertices: Vec<WorldModelRenderVertex>,
    indices: Vec<u32>,
    materials: Vec<WorldModelMaterial>,
    draws: Vec<WorldModelDrawCall>,
    shadow_draws: Vec<WorldModelDrawCall>,
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
        let mut shadow_draws = Vec::new();
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
            // 7D8379 copies MOGP's exterior count to group+60. Without MOCV
            // (7D7CE1, group flag 4), 7ABF50 selects 7AC6A0 and that count.
            // MOCV and root flag 2's MapObjU dispatch use the complete MOBA
            // count at group+16C instead (7AC9F0/7A9380).
            let visible_batch_count = if model.flags() & 2 == 0 && group.flags() & 4 == 0 {
                usize::from(group.batch_counts()[2])
            } else {
                group.batches().len()
            };
            if visible_batch_count > group.batches().len() {
                return Err(WorldModelMeshPlanError::DrawRange {
                    path: model.path().clone(),
                    group_index: group.index(),
                    batch_index: visible_batch_count,
                });
            }
            let first_draw = draws.len();
            let first_shadow_draw = shadow_draws.len();
            // 7AB760 consumes group+16C regardless of the ordinary surface
            // callback's exterior-only batch count. Both share these buffers.
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
                let draw = WorldModelDrawCall::new(
                    group.index(),
                    batch_first,
                    batch_count,
                    batch.material_id(),
                    batch.class(),
                    batch.bounds(),
                );
                shadow_draws.push(draw);
                if batch_index < visible_batch_count {
                    draws.push(draw);
                }
            }
            let group_shadows = &shadow_draws[first_shadow_draw..];
            if let Some(first) = group_shadows.first().copied()
                && group_shadows.iter().all(|draw| {
                    model.materials()[usize::from(draw.material_id())].blend_mode()
                        == WorldModelBlendMode::Opaque
                })
            {
                // 7D82E0 sets group+198 bit 4 only when every MOBA material
                // has blend zero. 7AB760 then draws the complete min/max
                // index span, including gaps between authored batch ranges.
                let mut low = first.first_index();
                let mut high = low + first.index_count();
                let mut bounds = first.bounds();
                for draw in group_shadows {
                    low = low.min(draw.first_index());
                    high = high.max(draw.first_index() + draw.index_count());
                    for (low, next) in bounds[0].iter_mut().zip(draw.bounds()[0]) {
                        *low = (*low).min(next);
                    }
                    for (high, next) in bounds[1].iter_mut().zip(draw.bounds()[1]) {
                        *high = (*high).max(next);
                    }
                }
                shadow_draws.truncate(first_shadow_draw);
                shadow_draws.push(WorldModelDrawCall::new(
                    group.index(),
                    low,
                    high - low,
                    first.material_id(),
                    first.class(),
                    bounds,
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
                [first_draw, draws.len()],
                [first_shadow_draw, shadow_draws.len()],
            ));
        }
        Ok(Self {
            path: model.path().clone(),
            root_flags: model.flags(),
            ambient_color: model.ambient_color(),
            vertices,
            indices,
            materials: model.materials().to_vec(),
            draws,
            shadow_draws,
            groups,
        })
    }

    /// Returns the canonical root-WMO resource identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns raw MOHD flags controlling the ordinary/unified effect family.
    #[must_use]
    pub const fn root_flags(&self) -> u16 {
        self.root_flags
    }

    /// Returns exact MOHD ambient BGRA bytes for unified interior passes.
    #[must_use]
    pub const fn ambient_color(&self) -> [u8; 4] {
        self.ambient_color
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

    /// Returns the ordinary callback's rebased MOBA draws in group/file order.
    #[must_use]
    pub fn draws(&self) -> &[WorldModelDrawCall] {
        &self.draws
    }

    /// Returns all MOBA shadow batches, merging each entirely opaque group
    /// into the original callback's complete minimum-to-maximum index span.
    #[must_use]
    pub fn shadow_draws(&self) -> &[WorldModelDrawCall] {
        &self.shadow_draws
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
    (0..group.vertices().len())
        .filter_map(|index| group.fixed_vertex_color(root_flags, index))
        .collect()
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
