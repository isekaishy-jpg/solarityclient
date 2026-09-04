//! Renderer-resident UI mesh, material, and sampled-image composition.

use std::collections::HashMap;

use solarity_asset::AssetPath;
use solarity_rendering::{
    BlpTextureHandle, UiFrameReport, UiGlyphTextureHandle, UiMeshHandle, UiMeshPlan,
    UiPreparedDraw, UiRenderBatch, UiRenderSource, UiSampledTexture, UiSamplerInfo, UiShaderSource,
    UiTextureResidency, VulkanError, VulkanRenderer,
};

use crate::application::ApplicationError;

/// One immutable UI mesh generation joined to renderer-owned resources.
pub(crate) struct PreparedUiFrame {
    mesh: UiMeshHandle,
    mesh_identity: u64,
    logical_extent: [f32; 2],
    /// All resident packets, including inactive retained Button state skins.
    resident_draws: Vec<UiPreparedDraw>,
    /// Visible packets submitted for the current live state.
    draws: Vec<UiPreparedDraw>,
    /// Source batch index parallel to each resident renderer draw.
    draw_batches: Vec<usize>,
    /// Complete material topology, including non-blocking unresolved sources.
    materials: Vec<UiRenderBatch>,
}

impl PreparedUiFrame {
    /// Uploads one complete mesh and prepares its ordered material packets.
    pub(crate) fn prepare(
        renderer: &mut VulkanRenderer,
        plan: &UiMeshPlan,
        textures: &HashMap<AssetPath, BlpTextureHandle>,
        glyph_texture: Option<(u64, UiGlyphTextureHandle)>,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_with_mesh(renderer, plan, textures, glyph_texture, None)
    }

    /// Rejoins changed material resources to an existing retained mesh slot.
    pub(crate) fn prepare_reusing_mesh(
        renderer: &mut VulkanRenderer,
        mesh: UiMeshHandle,
        plan: &UiMeshPlan,
        textures: &HashMap<AssetPath, BlpTextureHandle>,
        glyph_texture: Option<(u64, UiGlyphTextureHandle)>,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_with_mesh(renderer, plan, textures, glyph_texture, Some(mesh))
    }

    /// Builds one validated draw list around new or retained geometry storage.
    fn prepare_with_mesh(
        renderer: &mut VulkanRenderer,
        plan: &UiMeshPlan,
        textures: &HashMap<AssetPath, BlpTextureHandle>,
        glyph_texture: Option<(u64, UiGlyphTextureHandle)>,
        retained_mesh: Option<UiMeshHandle>,
    ) -> Result<Self, ApplicationError> {
        let mesh = if let Some(mesh) = retained_mesh {
            renderer.replace_ui_mesh(mesh, plan)?;
            mesh
        } else {
            renderer.upload_ui_mesh(plan)?
        };
        let mut batch_resources = Vec::with_capacity(plan.batches().len());
        let mut sampled_textures = Vec::new();
        for (batch_index, batch) in plan.batches().iter().enumerate() {
            let source = match batch.source() {
                UiRenderSource::Texture(_) | UiRenderSource::GlyphAtlas(_) => {
                    UiShaderSource::Texture
                }
                UiRenderSource::VertexColor => UiShaderSource::VertexColor,
            };
            let pipeline = renderer.prepare_ui_pipeline(source, batch.blend())?;
            let sampled_index = match batch.source() {
                UiRenderSource::Texture(path) => {
                    let Some(texture) = textures.get(path).copied() else {
                        if batch.residency() == UiTextureResidency::NonBlocking {
                            continue;
                        }
                        return Err(VulkanError::UiDrawTextureMismatch.into());
                    };
                    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
                        batch.horizontal_address(),
                        batch.vertical_address(),
                    ))?;
                    let index = sampled_textures.len();
                    sampled_textures.push(UiSampledTexture::new(texture, sampler));
                    Some(index)
                }
                UiRenderSource::GlyphAtlas(identity) => {
                    let (expected_identity, texture) =
                        glyph_texture.ok_or(VulkanError::UnknownUiGlyphTextureHandle)?;
                    if *identity != expected_identity {
                        return Err(VulkanError::UiDrawTextureMismatch.into());
                    }
                    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
                        batch.horizontal_address(),
                        batch.vertical_address(),
                    ))?;
                    let index = sampled_textures.len();
                    sampled_textures.push(UiSampledTexture::glyph(texture, sampler));
                    Some(index)
                }
                UiRenderSource::VertexColor => None,
            };
            batch_resources.push((batch_index, pipeline, sampled_index));
        }
        let texture_sets = renderer.prepare_ui_texture_sets(&sampled_textures)?;
        let draw_batches = batch_resources
            .iter()
            .map(|(batch_index, _pipeline, _sampled)| *batch_index)
            .collect();
        let resident_draws = batch_resources
            .into_iter()
            .map(
                |(batch_index, pipeline, sampled_index)| -> Result<_, ApplicationError> {
                    let mut draw = renderer.prepare_ui_draw(
                        mesh,
                        pipeline,
                        sampled_index.map(|index| texture_sets[index]),
                        plan,
                        batch_index,
                    )?;
                    apply_clipped_batch_range(&mut draw, plan, batch_index)?;
                    Ok(draw)
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let draws = resident_draws
            .iter()
            .copied()
            .filter(|draw| draw.opacity() > 0.0 && draw.index_count() > 0)
            .collect();
        Ok(Self {
            mesh,
            mesh_identity: plan.geometry_identity(),
            logical_extent: plan.logical_extent(),
            resident_draws,
            draws,
            draw_batches,
            materials: plan.batches().to_vec(),
        })
    }

    /// Presents this generation followed by one independently retained overlay.
    pub(crate) fn present_with_overlay(
        &self,
        renderer: &mut VulkanRenderer,
        overlay: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, ApplicationError> {
        renderer
            .present_ui_with_overlay(self.logical_extent, &self.draws, overlay)
            .map_err(ApplicationError::from)
    }

    /// Replaces this frame's mesh while retaining compatible material resources.
    pub(crate) fn replace_mesh(
        &mut self,
        renderer: &mut VulkanRenderer,
        plan: &UiMeshPlan,
    ) -> Result<(), ApplicationError> {
        if self.refresh_retained_draw_state(plan)? {
            return Ok(());
        }
        if !self.can_replace_mesh(plan) {
            return Err(VulkanError::UiDrawIndex {
                requested: plan.batches().len(),
                available: self.materials.len(),
            }
            .into());
        }
        let draw_topology_unchanged = self
            .materials
            .iter()
            .zip(plan.batches())
            .all(|(retained, candidate)| same_ui_draw_topology(retained, candidate));
        let mesh = self.mesh;
        if self.mesh_identity != plan.geometry_identity() {
            renderer.replace_ui_mesh(mesh, plan)?;
            self.mesh_identity = plan.geometry_identity();
        }
        if draw_topology_unchanged {
            // Color/position byte patches keep every indexed range and
            // material binding stable. Revalidating and reconstructing every
            // draw in a several-thousand-packet Glue frame made one hover
            // mutation proportional to the whole UI catalog.
            for (draw, batch_index) in self.resident_draws.iter_mut().zip(&self.draw_batches) {
                let batch = &plan.batches()[*batch_index];
                draw.set_transform_state(batch.translation(), batch.opacity(), batch.clip());
                apply_clipped_batch_range(draw, plan, *batch_index)?;
            }
        } else {
            for (draw, batch_index) in self.resident_draws.iter_mut().zip(&self.draw_batches) {
                *draw = renderer.prepare_ui_draw(
                    mesh,
                    draw.pipeline(),
                    draw.texture_set(),
                    plan,
                    *batch_index,
                )?;
                apply_clipped_batch_range(draw, plan, *batch_index)?;
            }
        }
        self.materials.clear();
        self.materials.extend_from_slice(plan.batches());
        self.refresh_visible_draws();
        self.logical_extent = plan.logical_extent();
        Ok(())
    }

    /// Replaces only geometry when every prepared batch still selects the
    /// same material in the same draw slot. A caller can fall back to complete
    /// resource preparation when visibility changes the material topology.
    pub(crate) fn try_replace_compatible_mesh(
        &mut self,
        renderer: &mut VulkanRenderer,
        plan: &UiMeshPlan,
    ) -> Result<bool, ApplicationError> {
        if self.refresh_retained_draw_state(plan)? {
            return Ok(true);
        }
        if !self.can_replace_mesh(plan) {
            return Ok(false);
        }
        self.replace_mesh(renderer, plan)?;
        Ok(true)
    }

    /// Updates only push constants and scissors when CPU and GPU share geometry.
    fn refresh_retained_draw_state(&mut self, plan: &UiMeshPlan) -> Result<bool, ApplicationError> {
        if self.mesh_identity != plan.geometry_identity() {
            return Ok(false);
        }
        if self.materials.len() != plan.batches().len() {
            return Ok(false);
        }
        for (draw, batch_index) in self.resident_draws.iter_mut().zip(&self.draw_batches) {
            let Some(batch) = plan.batches().get(*batch_index) else {
                return Ok(false);
            };
            draw.set_transform_state(batch.translation(), batch.opacity(), batch.clip());
            apply_clipped_batch_range(draw, plan, *batch_index)?;
        }
        self.refresh_visible_draws();
        self.logical_extent = plan.logical_extent();
        Ok(true)
    }

    fn refresh_visible_draws(&mut self) {
        self.draws.clear();
        self.draws.extend(
            self.resident_draws
                .iter()
                .copied()
                .filter(|draw| draw.opacity() > 0.0 && draw.index_count() > 0),
        );
    }

    fn can_replace_mesh(&self, plan: &UiMeshPlan) -> bool {
        self.materials.len() == plan.batches().len()
            && self
                .materials
                .iter()
                .zip(plan.batches())
                .all(|(retained, candidate)| same_ui_material(retained, candidate))
    }

    pub(crate) const fn logical_extent(&self) -> [f32; 2] {
        self.logical_extent
    }

    pub(crate) fn draws(&self) -> &[UiPreparedDraw] {
        &self.draws
    }

    /// Returns the stable renderer allocation reused across material changes.
    pub(crate) const fn mesh(&self) -> UiMeshHandle {
        self.mesh
    }
}

fn apply_clipped_batch_range(
    draw: &mut UiPreparedDraw,
    plan: &UiMeshPlan,
    batch_index: usize,
) -> Result<(), ApplicationError> {
    let Some((first_quad, quad_count)) = plan.clipped_batch_quad_range(batch_index) else {
        return Err(VulkanError::UiDrawIndex {
            requested: batch_index,
            available: plan.batches().len(),
        }
        .into());
    };
    if !draw.set_quad_range(first_quad, quad_count) {
        return Err(VulkanError::UiDrawIndexRange.into());
    }
    Ok(())
}

fn same_ui_material(left: &UiRenderBatch, right: &UiRenderBatch) -> bool {
    left.source() == right.source()
        && left.blend() == right.blend()
        && left.horizontal_address() == right.horizontal_address()
        && left.vertical_address() == right.vertical_address()
        && left.residency() == right.residency()
        && left.desaturated() == right.desaturated()
}

fn same_ui_draw_topology(left: &UiRenderBatch, right: &UiRenderBatch) -> bool {
    left.first_index() == right.first_index()
        && left.index_count() == right.index_count()
        && left.first_quad() == right.first_quad()
}
