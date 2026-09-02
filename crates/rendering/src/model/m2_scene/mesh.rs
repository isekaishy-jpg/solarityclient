//! Shared CPU mesh preparation from one explicit external SKIN profile.

use solarity_asset::{AssetPath, DecodedM2Model, M2Batch, M2BlendMode, M2SkinProfile};

use crate::model::character_component::CharacterGeosetPlan;

use super::{M2DrawCall, M2MeshPlanError, M2RenderVertex, M2TextureBinding};

/// Upload-ready shared geometry and material draws for one M2 view profile.
#[derive(Clone, Debug, PartialEq)]
pub struct M2MeshPlan {
    path: AssetPath,
    profile_index: usize,
    vertices: Vec<M2RenderVertex>,
    indices: Vec<u16>,
    draws: Vec<M2DrawCall>,
}

impl M2MeshPlan {
    /// Resolves the selected SKIN indirection and every material batch.
    ///
    /// Profile choice is explicit because view/LOD policy belongs to the scene,
    /// not asset decoding. The function never substitutes another profile.
    ///
    /// # Errors
    ///
    /// Returns [`M2MeshPlanError`] when the profile is absent or any cross-array
    /// batch, material, texture, or coordinate reference is invalid.
    pub fn prepare(model: &DecodedM2Model, profile_index: usize) -> Result<Self, M2MeshPlanError> {
        let profile =
            model
                .skins()
                .get(profile_index)
                .ok_or_else(|| M2MeshPlanError::MissingProfile {
                    path: model.path().clone(),
                    profile_index,
                })?;
        let vertices = resolve_vertices(model, profile)?;
        let indices = resolve_indices(profile)?;
        let draws = resolve_draws(model, profile)?;
        Ok(Self {
            path: model.path().clone(),
            profile_index,
            vertices,
            indices,
            draws,
        })
    }

    /// Returns the normalized model path used as a renderer cache identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the explicit zero-based SKIN profile selected by the scene.
    #[must_use]
    pub const fn profile_index(&self) -> usize {
        self.profile_index
    }

    /// Returns fixed-layout vertices ready for a shared GPU resource.
    #[must_use]
    pub fn vertices(&self) -> &[M2RenderVertex] {
        &self.vertices
    }

    /// Returns direct M2 vertex indices after resolving SKIN indirection.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns material batches in exact external SKIN order.
    #[must_use]
    pub fn draws(&self) -> &[M2DrawCall] {
        &self.draws
    }

    /// Reports whether the selected profile owns an ordinary mesh draw.
    ///
    /// Stock effect models may contain particles or ribbons with an empty
    /// SKIN. Those models remain present in the scene but need no static
    /// vertex/index allocation.
    #[must_use]
    pub fn has_drawable_geometry(&self) -> bool {
        !self.draws.is_empty()
    }

    /// Returns the greatest model-bone index referenced by a nonzero weight.
    #[must_use]
    pub fn max_bone_index(&self) -> Option<u16> {
        self.vertices
            .iter()
            .flat_map(|vertex| vertex.bone_weights().into_iter().zip(vertex.bone_indices()))
            .filter_map(|(weight, index)| (weight != 0).then_some(index))
            .max()
    }

    /// Iterates only draws whose character geoset remains enabled.
    pub fn character_draws<'plan>(
        &'plan self,
        geosets: &'plan CharacterGeosetPlan,
    ) -> impl Iterator<Item = &'plan M2DrawCall> + 'plan {
        self.draws
            .iter()
            .filter(|draw| geosets.is_visible(draw.geoset_id()))
    }

    /// Marks bones weighted by the selected character-eye surface.
    ///
    /// Character 17xx geometry uses authored billboard flags for its eye-card
    /// implementation, but build 12340's character compositor keeps those
    /// selected bones oriented with the face. The returned table is indexed
    /// by the model's bone array and deliberately excludes hidden eye variants.
    #[must_use]
    pub fn character_eye_orientation_mask(
        &self,
        geosets: &CharacterGeosetPlan,
        bone_count: usize,
    ) -> Vec<bool> {
        let mut mask = vec![false; bone_count];
        for draw in self
            .character_draws(geosets)
            .filter(|draw| draw.geoset_id() / 100 == 17)
        {
            let first = draw.first_index() as usize;
            let end = first.saturating_add(draw.index_count() as usize);
            let Some(indices) = self.indices.get(first..end) else {
                debug_assert!(false, "validated M2 draw range left the index table");
                continue;
            };
            for vertex_index in indices.iter().copied().map(usize::from) {
                let Some(vertex) = self.vertices.get(vertex_index) else {
                    debug_assert!(false, "validated M2 index left the vertex table");
                    continue;
                };
                for (weight, bone) in vertex.bone_weights().into_iter().zip(vertex.bone_indices()) {
                    if weight != 0
                        && let Some(entry) = mask.get_mut(usize::from(bone))
                    {
                        *entry = true;
                    }
                }
            }
        }
        mask
    }

    /// Serializes vertices without relying on Rust layout or unsafe casts.
    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * M2RenderVertex::BYTE_SIZE);
        for vertex in &self.vertices {
            vertex.append_bytes(&mut bytes);
        }
        bytes
    }

    /// Serializes resolved unsigned-short indices for Vulkan upload.
    #[must_use]
    pub fn index_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.indices.len() * size_of::<u16>());
        for index in &self.indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes
    }
}

/// Resolves triangle entries through the profile-local vertex lookup once.
fn resolve_indices(profile: &M2SkinProfile) -> Result<Vec<u16>, M2MeshPlanError> {
    let mut indices = Vec::with_capacity(profile.triangle_lookup().len());
    for (triangle_index, profile_vertex) in profile.triangle_lookup().iter().copied().enumerate() {
        if usize::from(profile_vertex) >= profile.vertex_lookup().len() {
            return Err(M2MeshPlanError::MissingProfileVertex {
                path: profile.path().clone(),
                triangle_index,
                profile_vertex,
            });
        }
        indices.push(profile_vertex);
    }
    Ok(indices)
}

/// Copies the selected SKIN vertex domain and resolves its replacement palettes.
fn resolve_vertices(
    model: &DecodedM2Model,
    profile: &M2SkinProfile,
) -> Result<Vec<M2RenderVertex>, M2MeshPlanError> {
    let mut owners = vec![None; profile.vertex_lookup().len()];
    for submesh in profile.submeshes() {
        let start = usize::from(submesh.vertex_start);
        let end = start + usize::from(submesh.vertex_count);
        for owner in &mut owners[start..end] {
            // Build 12340 applies sections in file order; a later overlapping
            // section therefore owns the final palette replacement.
            *owner = Some(submesh);
        }
    }

    let mut vertices = Vec::with_capacity(profile.vertex_lookup().len());
    for (profile_vertex, model_vertex) in profile.vertex_lookup().iter().copied().enumerate() {
        let vertex = model.vertices()[usize::from(model_vertex)];
        let mut weights = [0_u8; 4];
        let mut indices = [0_u16; 4];
        if let Some(submesh) = owners[profile_vertex] {
            let influence_count = match submesh.bone_influence {
                0 => 0,
                1 => 1,
                2..=u16::MAX => 4,
            };
            let source_weights = vertex.bone_weights();
            for influence in 0..influence_count {
                let weight = source_weights[influence];
                if weight == 0 {
                    continue;
                }
                let palette = usize::from(profile.bone_indices()[profile_vertex * 4 + influence]);
                let lookup_index = usize::from(submesh.bone_start) + palette;
                let bone = model
                    .bone_lookup()
                    .get(lookup_index)
                    .copied()
                    .ok_or_else(|| M2MeshPlanError::MissingBoneLookup {
                        path: profile.path().clone(),
                        profile_vertex,
                        influence,
                        lookup_index,
                    })?;
                weights[influence] = weight;
                indices[influence] = bone;
            }
        }
        vertices.push(M2RenderVertex::from_profile(vertex, weights, indices));
    }
    Ok(vertices)
}

/// Translates every batch while preserving its shader and animation selectors.
fn resolve_draws(
    model: &DecodedM2Model,
    profile: &M2SkinProfile,
) -> Result<Vec<M2DrawCall>, M2MeshPlanError> {
    let mut draws = Vec::with_capacity(profile.batches().len());
    for (batch_index, batch) in profile.batches().iter().copied().enumerate() {
        let submesh = profile
            .submeshes()
            .get(usize::from(batch.skin_section_index))
            .ok_or_else(|| M2MeshPlanError::MissingSubmesh {
                path: profile.path().clone(),
                batch_index,
                submesh_index: batch.skin_section_index,
            })?;
        let material = model
            .materials()
            .get(usize::from(batch.material_index))
            .copied()
            .ok_or_else(|| M2MeshPlanError::MissingMaterial {
                path: profile.path().clone(),
                batch_index,
                material_index: batch.material_index,
            })?;
        let base_material_index = batch
            .material_index
            .checked_sub(batch.material_layer)
            .ok_or_else(|| M2MeshPlanError::MissingBaseMaterial {
                path: profile.path().clone(),
                batch_index,
                material_index: batch.material_index,
                material_layer: batch.material_layer,
            })?;
        let base_material = model
            .materials()
            .get(usize::from(base_material_index))
            .ok_or_else(|| M2MeshPlanError::MissingBaseMaterial {
                path: profile.path().clone(),
                batch_index,
                material_index: batch.material_index,
                material_layer: batch.material_layer,
            })?;
        let texture_bindings = resolve_texture_bindings(model, profile, batch_index, batch)?;
        let first_index = u32::from(submesh.triangle_start) + (u32::from(submesh.level) << 16);
        draws.push(M2DrawCall::new(
            *submesh,
            first_index,
            u32::from(submesh.triangle_count),
            batch,
            material,
            !matches!(
                base_material.blend_mode(),
                M2BlendMode::Opaque | M2BlendMode::AlphaKey
            ),
            texture_bindings,
        ));
    }
    Ok(draws)
}

/// Resolves texture and coordinate combo tables for one material batch.
fn resolve_texture_bindings(
    model: &DecodedM2Model,
    profile: &M2SkinProfile,
    batch_index: usize,
    batch: M2Batch,
) -> Result<Vec<M2TextureBinding>, M2MeshPlanError> {
    let mut bindings = Vec::with_capacity(usize::from(batch.texture_count));
    for stage in 0..batch.texture_count {
        let texture_combo = usize::from(batch.texture_combo_index) + usize::from(stage);
        let texture_index = model
            .texture_lookup()
            .get(texture_combo)
            .copied()
            .ok_or_else(|| M2MeshPlanError::MissingTextureCombo {
                path: profile.path().clone(),
                batch_index,
                stage,
                combo_index: texture_combo,
            })?;
        if usize::from(texture_index) >= model.textures().len() {
            return Err(M2MeshPlanError::MissingTexture {
                path: profile.path().clone(),
                batch_index,
                stage,
                texture_index,
            });
        }
        let coordinate_combo =
            usize::from(batch.texture_coordinate_combo_index) + usize::from(stage);
        let texture_coordinate = model
            .texture_coordinate_lookup()
            .get(coordinate_combo)
            .copied()
            .ok_or_else(|| M2MeshPlanError::MissingCoordinateCombo {
                path: profile.path().clone(),
                batch_index,
                stage,
                combo_index: coordinate_combo,
            })?;
        bindings.push(M2TextureBinding::new(
            stage,
            texture_index,
            texture_coordinate,
        ));
    }
    Ok(bindings)
}
