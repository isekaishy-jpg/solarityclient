//! GPU source admission validates immutable draw templates and retains resource leases.

use super::super::{
    Arc, BlpColorSpace, BlpTextureUploadRequest, DecodedM2Model, M2CpuSource, M2GeosetSelection,
    M2GpuDraw, M2GpuParticle, M2GpuRibbonPass, M2GpuSource, M2LocalLightCount, M2MaterialState,
    M2MeshPlan, M2ModelOrientation, M2ParticleEmitter, M2ParticlePipelineHandle, M2ResolvedTexture,
    M2SampledTexture, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation,
    M2SpirvKey, M2TextureImageHandle, M2TextureSet, ResidentM2Source, ResidentM2Texture,
    RuntimeTerrainFrameError, VulkanRenderer, prepare_m2_cpu_source,
};

/// Publishes a source only when every selected draw has concrete BLP stages.
pub(in super::super) fn prepare_source(
    renderer: &mut VulkanRenderer,
    source: &ResidentM2Source,
) -> Result<Option<M2GpuSource>, RuntimeTerrainFrameError> {
    prepare_source_with_lights(renderer, source, M2LocalLightCount::Four)
}

pub(in super::super) fn prepare_source_with_lights(
    renderer: &mut VulkanRenderer,
    source: &ResidentM2Source,
    local_light_count: M2LocalLightCount,
) -> Result<Option<M2GpuSource>, RuntimeTerrainFrameError> {
    let model = source.model();
    if source.textures().len() != model.textures().len() {
        return Err(RuntimeTerrainFrameError::M2TextureTableCount {
            model: model.path().clone(),
            source_count: source.textures().len(),
            model_count: model.textures().len(),
        });
    }
    let resolved = source
        .textures()
        .iter()
        .map(|texture| match texture {
            ResidentM2Texture::Authored(source) => M2ResolvedTexture::Authored(source.as_ref()),
            ResidentM2Texture::StockWhite => M2ResolvedTexture::StockWhite,
            ResidentM2Texture::StockFailure => M2ResolvedTexture::StockFailure,
            ResidentM2Texture::Replaceable(kind) => M2ResolvedTexture::Unresolved(*kind),
        })
        .collect::<Vec<_>>();
    let plan = Arc::clone(&source.cpu_source().plan);
    for draw in plan.draws() {
        for binding in draw.texture_bindings() {
            let texture = resolved
                .get(usize::from(binding.texture_index()))
                .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: binding.texture_index(),
                })?;
            if let M2ResolvedTexture::Unresolved(kind) = texture {
                tracing::debug!(
                    path = %model.path(),
                    ?kind,
                    "static world M2 omitted until its replacement texture is supplied"
                );
                return Ok(None);
            }
        }
    }
    for emitter in model.animations().ribbons() {
        for texture_index in emitter.texture_indices() {
            let texture = resolved.get(usize::from(*texture_index)).ok_or_else(|| {
                RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: *texture_index,
                }
            })?;
            if let M2ResolvedTexture::Unresolved(kind) = texture {
                tracing::debug!(
                    path = %model.path(),
                    ?kind,
                    "static world M2 omitted until its ribbon replacement texture is supplied"
                );
                return Ok(None);
            }
        }
    }
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        let texture = resolved.get(usize::from(texture_index)).ok_or_else(|| {
            RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index,
            }
        })?;
        if let M2ResolvedTexture::Unresolved(kind) = texture {
            tracing::debug!(
                path = %model.path(),
                particle_index,
                ?kind,
                "static world M2 omitted until its particle replacement texture is supplied"
            );
            return Ok(None);
        }
    }
    Ok(Some(prepare_gpu_source_from_cpu(
        renderer,
        model,
        &resolved,
        None,
        local_light_count,
        source.cpu_source(),
        M2ModelOrientation::Authored,
    )?))
}

/// Publishes immutable model resources using one owner's resolved texture table.
pub(in super::super) fn prepare_gpu_source(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    let cpu_source = prepare_m2_cpu_source(model, local_light_count)?;
    prepare_gpu_source_from_cpu(
        renderer,
        model,
        textures,
        geosets,
        local_light_count,
        &cpu_source,
        orientation,
    )
}

/// Publishes a Glue source from worker-prepared mesh and shader state.
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn prepare_gpu_source_from_cpu(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    cpu_source: &M2CpuSource,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    prepare_gpu_source_with_plan(
        renderer,
        model,
        textures,
        geosets,
        local_light_count,
        Arc::clone(&cpu_source.plan),
        Some(cpu_source),
        orientation,
    )
}

/// Joins one CPU plan to texture uploads and render-owner Vulkan resources.
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn prepare_gpu_source_with_plan(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    plan: Arc<M2MeshPlan>,
    cpu_source: Option<&M2CpuSource>,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    let mut profile = solarity_profiling::profile!("M2 source publication");
    if textures.len() != model.textures().len() {
        return Err(RuntimeTerrainFrameError::M2TextureTableCount {
            model: model.path().clone(),
            source_count: textures.len(),
            model_count: model.textures().len(),
        });
    }
    validate_gpu_texture_coverage(model, &plan, textures, geosets)?;
    let model_oriented_billboard_bones = match geosets {
        Some(M2GeosetSelection::Character(geosets)) => {
            plan.character_eye_orientation_mask(geosets, model.animations().bones().len())
        }
        Some(M2GeosetSelection::Creature(_)) | None => Vec::new(),
    };
    let mut upload_indices = Vec::new();
    let mut uploads = Vec::new();
    for (texture_index, texture) in textures.iter().enumerate() {
        if let M2ResolvedTexture::Authored(source_texture) = texture {
            upload_indices.push(texture_index);
            uploads.push(BlpTextureUploadRequest::new(
                source_texture,
                // Build 12340's fixed-function M2 combiners multiply and add
                // sampled byte values directly. An sRGB image view would
                // linearize them first, darkening smoke and suppressing glow.
                BlpColorSpace::Linear,
            ));
        }
    }
    let uploaded = renderer.upload_blp_textures(&uploads)?;
    profile.mark("validation and BLP uploads");
    let mut texture_handles = vec![None; textures.len()];
    for (texture_index, handle) in upload_indices.into_iter().zip(uploaded) {
        texture_handles[texture_index] = Some(M2TextureImageHandle::Blp(handle));
    }
    let mut resource_leases = Vec::new();
    let mut atlas_handle = None;
    for (texture_index, texture) in textures.iter().enumerate() {
        if let M2ResolvedTexture::CharacterAtlas(atlas) = texture {
            let handle = match atlas_handle {
                Some(handle) => handle,
                None => {
                    let handle = renderer.upload_character_atlas_texture(atlas)?;
                    resource_leases.push(renderer.retain_character_atlas(handle)?);
                    atlas_handle = Some(handle);
                    handle
                }
            };
            texture_handles[texture_index] = Some(M2TextureImageHandle::CharacterAtlas(handle));
        }
    }
    let stock_white = textures
        .iter()
        .any(|texture| matches!(texture, M2ResolvedTexture::StockWhite))
        .then(|| renderer.upload_stock_m2_white())
        .transpose()?;
    let stock_failure = textures
        .iter()
        .any(|texture| matches!(texture, M2ResolvedTexture::StockFailure))
        .then(|| renderer.upload_stock_m2_failure())
        .transpose()?;
    for (texture_index, texture) in textures.iter().enumerate() {
        let handle = match texture {
            M2ResolvedTexture::StockWhite => stock_white,
            M2ResolvedTexture::StockFailure => stock_failure,
            M2ResolvedTexture::Authored(_)
            | M2ResolvedTexture::CharacterAtlas(_)
            | M2ResolvedTexture::Unresolved(_) => None,
        };
        if let Some(handle) = handle {
            texture_handles[texture_index] = Some(M2TextureImageHandle::Blp(handle));
        }
    }

    profile.mark("special texture uploads");
    let mesh = if plan.has_drawable_geometry() {
        {
            let handle = renderer.upload_m2_mesh(&plan)?;
            resource_leases.push(renderer.retain_m2_mesh(handle)?);
            Some(handle)
        }
    } else {
        None
    };
    profile.mark("mesh upload");
    let mut texture_requests = Vec::with_capacity(plan.draws().len());
    let mut pipelines = Vec::with_capacity(plan.draws().len());
    for (draw_index, draw) in plan.draws().iter().enumerate() {
        if geosets.is_some_and(|geosets| !geosets.is_visible(draw.geoset_id())) {
            continue;
        }
        let shader = M2ShaderPlan::resolve(model, draw)?;
        let permutation = M2ShaderPermutation::resolve(
            draw,
            local_light_count,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        );
        let pipeline = match cpu_source {
            Some(cpu_source) => {
                let program = cpu_source
                    .mesh_programs
                    .get(&M2SpirvKey::new(shader, permutation))
                    .ok_or_else(|| RuntimeTerrainFrameError::M2CpuProgram {
                        model: model.path().clone(),
                        domain: "mesh",
                    })?;
                renderer.prepare_precompiled_oriented_m2_pipeline(program, orientation)?
            }
            None => renderer.prepare_oriented_m2_pipeline(shader, permutation, orientation)?,
        };
        let material = M2MaterialState::from_material(draw.material());
        let runtime_fade_pipeline = if material.blend_enabled() {
            None
        } else {
            let fade = shader.with_runtime_alpha_fade();
            Some(match cpu_source {
                Some(cpu_source) => {
                    let program = cpu_source
                        .mesh_programs
                        .get(&M2SpirvKey::new(fade, permutation))
                        .ok_or_else(|| RuntimeTerrainFrameError::M2CpuProgram {
                            model: model.path().clone(),
                            domain: "runtime-fade mesh",
                        })?;
                    renderer.prepare_precompiled_oriented_m2_pipeline(program, orientation)?
                }
                None => renderer.prepare_oriented_m2_pipeline(fade, permutation, orientation)?,
            })
        };
        pipelines.push((draw_index, pipeline, runtime_fade_pipeline));

        let mut stages = Vec::with_capacity(draw.texture_bindings().len());
        for binding in draw.texture_bindings() {
            let texture_index = usize::from(binding.texture_index());
            let texture = require_texture_handle(model, textures, &texture_handles, texture_index)?;
            let sampler = renderer.prepare_m2_file_sampler(&model.textures()[texture_index])?;
            stages.push(sampled_texture(texture, sampler));
        }
        texture_requests.push(texture_set(model, draw_index, &stages)?);
    }
    profile.mark("mesh pipelines and samplers");
    let texture_sets = renderer.prepare_m2_texture_sets(&texture_requests)?;
    profile.mark("mesh descriptors");
    let mut draws = Vec::with_capacity(plan.draws().len());
    draws.resize_with(plan.draws().len(), || None);
    for ((draw_index, pipeline, runtime_fade_pipeline), texture_set) in
        pipelines.into_iter().zip(texture_sets)
    {
        let mesh = mesh.ok_or(solarity_rendering::VulkanError::UnknownM2MeshHandle)?;
        let template = renderer.prepare_m2_draw_template(
            mesh,
            pipeline,
            texture_set,
            &plan,
            draw_index,
            false,
        )?;
        let fade_template = runtime_fade_pipeline
            .map(|pipeline| {
                renderer.prepare_m2_draw_template(
                    mesh,
                    pipeline,
                    texture_set,
                    &plan,
                    draw_index,
                    true,
                )
            })
            .transpose()?;
        draws[draw_index] = Some(M2GpuDraw {
            template,
            fade_template,
            pipeline,
            texture_set,
        });
    }
    let mut particle_pipelines = Vec::with_capacity(model.animations().particles().len());
    let mut particle_texture_requests = Vec::with_capacity(model.animations().particles().len());
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        let texture_slot = usize::from(texture_index);
        let texture = require_texture_handle(model, textures, &texture_handles, texture_slot)?;
        let sampler = renderer.prepare_m2_file_sampler(&model.textures()[texture_slot])?;
        let material = M2MaterialState::from_particle(emitter.blending_type(), emitter.flags());
        let mut prepare_pipeline =
            |material| -> Result<M2ParticlePipelineHandle, RuntimeTerrainFrameError> {
                match cpu_source {
                    Some(cpu_source) => {
                        let program =
                            cpu_source.particle_programs.get(&material).ok_or_else(|| {
                                RuntimeTerrainFrameError::M2CpuProgram {
                                    model: model.path().clone(),
                                    domain: "particle",
                                }
                            })?;
                        renderer
                            .prepare_precompiled_m2_particle_pipeline(program)
                            .map_err(Into::into)
                    }
                    None => renderer
                        .prepare_m2_particle_pipeline(material)
                        .map_err(Into::into),
                }
            };
        let pipeline = prepare_pipeline(material)?;
        let runtime_fade_pipeline = if material.blend_enabled() {
            pipeline
        } else {
            prepare_pipeline(material.with_runtime_alpha_fade())?
        };
        particle_pipelines.push((pipeline, runtime_fade_pipeline));
        particle_texture_requests.push(M2TextureSet::One(sampled_texture(texture, sampler)));
    }
    let particle_texture_sets = renderer.prepare_m2_texture_sets(&particle_texture_requests)?;
    let particles = particle_pipelines
        .into_iter()
        .zip(particle_texture_sets)
        .map(|((pipeline, runtime_fade_pipeline), texture_set)| {
            Ok::<_, solarity_rendering::VulkanError>(M2GpuParticle {
                template: renderer
                    .m2_effect_draw_catalog()
                    .particle_template(pipeline, texture_set)?,
                fade_template: renderer
                    .m2_effect_draw_catalog()
                    .particle_template(runtime_fade_pipeline, texture_set)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut ribbons = Vec::with_capacity(model.animations().ribbons().len());
    for emitter in model.animations().ribbons() {
        let mut pass_pipelines = Vec::with_capacity(emitter.material_indices().len());
        let mut pass_textures = Vec::with_capacity(emitter.texture_indices().len());
        for (material_index, texture_index) in emitter
            .material_indices()
            .iter()
            .copied()
            .zip(emitter.texture_indices().iter().copied())
        {
            let material = model.materials()[usize::from(material_index)];
            let pipeline = match cpu_source {
                Some(cpu_source) => {
                    let state = M2MaterialState::from_material(material);
                    let program = cpu_source.ribbon_programs.get(&state).ok_or_else(|| {
                        RuntimeTerrainFrameError::M2CpuProgram {
                            model: model.path().clone(),
                            domain: "ribbon",
                        }
                    })?;
                    renderer.prepare_precompiled_m2_ribbon_pipeline(program)?
                }
                None => renderer.prepare_m2_ribbon_pipeline(material)?,
            };
            let texture_slot = usize::from(texture_index);
            let texture = require_texture_handle(model, textures, &texture_handles, texture_slot)?;
            let sampler = renderer.prepare_m2_file_sampler(&model.textures()[texture_slot])?;
            pass_pipelines.push((pipeline, material));
            pass_textures.push(M2TextureSet::One(sampled_texture(texture, sampler)));
        }
        let texture_sets = renderer.prepare_m2_texture_sets(&pass_textures)?;
        ribbons.push(
            pass_pipelines
                .into_iter()
                .zip(texture_sets)
                .map(|((pipeline, material), texture_set)| {
                    Ok::<_, solarity_rendering::VulkanError>(M2GpuRibbonPass {
                        template: renderer
                            .m2_effect_draw_catalog()
                            .ribbon_template(pipeline, texture_set)?,
                        material,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    profile.mark("effects");
    Ok(Arc::new(super::super::M2GpuSourceData {
        _resource_leases: resource_leases,
        model: Arc::clone(model),
        plan,
        model_oriented_billboard_bones,
        animated_shadow_caster: model
            .animations()
            .bones()
            .iter()
            .any(|bone| bone.flags() & 0x2f8 != 0),
        mesh,
        draws,
        particles,
        ribbons,
    }))
}

/// Rejects selected holes before any renderer registry mutates.
pub(in super::super) fn validate_gpu_texture_coverage(
    model: &DecodedM2Model,
    plan: &M2MeshPlan,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
) -> Result<(), RuntimeTerrainFrameError> {
    for draw in plan.draws() {
        if geosets.is_some_and(|geosets| !geosets.is_visible(draw.geoset_id())) {
            continue;
        }
        for binding in draw.texture_bindings() {
            require_resolved_texture(model, textures, usize::from(binding.texture_index()))?;
        }
    }
    for emitter in model.animations().ribbons() {
        for texture_index in emitter.texture_indices() {
            require_resolved_texture(model, textures, usize::from(*texture_index))?;
        }
    }
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        require_resolved_texture(model, textures, usize::from(texture_index))?;
    }
    Ok(())
}

/// Verifies that one selected slot has an owner-provided image source.
pub(in super::super) fn require_resolved_texture(
    model: &DecodedM2Model,
    textures: &[M2ResolvedTexture<'_>],
    texture_index: usize,
) -> Result<(), RuntimeTerrainFrameError> {
    match textures.get(texture_index) {
        Some(
            M2ResolvedTexture::Authored(_)
            | M2ResolvedTexture::CharacterAtlas(_)
            | M2ResolvedTexture::StockWhite
            | M2ResolvedTexture::StockFailure,
        ) => Ok(()),
        Some(M2ResolvedTexture::Unresolved(kind)) => {
            Err(RuntimeTerrainFrameError::M2UnresolvedTexture {
                model: model.path().clone(),
                texture_index,
                kind: *kind,
            })
        }
        None => {
            let texture_index = u16::try_from(texture_index).map_err(|_source| {
                RuntimeTerrainFrameError::M2TextureIndexCapacity {
                    model: model.path().clone(),
                    texture_index,
                }
            })?;
            Err(RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index,
            })
        }
    }
}

/// Requires one resolved image handle for a selected visible/effect texture.
pub(in super::super) fn require_texture_handle(
    model: &DecodedM2Model,
    textures: &[M2ResolvedTexture<'_>],
    handles: &[Option<M2TextureImageHandle>],
    texture_index: usize,
) -> Result<M2TextureImageHandle, RuntimeTerrainFrameError> {
    let diagnostic_index = u16::try_from(texture_index).map_err(|_source| {
        RuntimeTerrainFrameError::M2TextureIndexCapacity {
            model: model.path().clone(),
            texture_index,
        }
    })?;
    let texture =
        textures
            .get(texture_index)
            .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index: diagnostic_index,
            })?;
    match texture {
        M2ResolvedTexture::Unresolved(kind) => Err(RuntimeTerrainFrameError::M2UnresolvedTexture {
            model: model.path().clone(),
            texture_index,
            kind: *kind,
        }),
        M2ResolvedTexture::Authored(_)
        | M2ResolvedTexture::CharacterAtlas(_)
        | M2ResolvedTexture::StockWhite
        | M2ResolvedTexture::StockFailure => handles
            .get(texture_index)
            .copied()
            .flatten()
            .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index: diagnostic_index,
            }),
    }
}

/// Couples a typed image source to the model declaration's stock sampler.
pub(in super::super) fn sampled_texture(
    image: M2TextureImageHandle,
    sampler: solarity_rendering::M2SamplerHandle,
) -> M2SampledTexture {
    match image {
        M2TextureImageHandle::Blp(handle) => M2SampledTexture::new(handle, sampler),
        M2TextureImageHandle::CharacterAtlas(handle) => {
            M2SampledTexture::character_atlas(handle, sampler)
        }
    }
}

/// Closes the stock shader's one-or-two-stage material domain without panic.
pub(in super::super) fn texture_set(
    model: &DecodedM2Model,
    draw_index: usize,
    stages: &[M2SampledTexture],
) -> Result<M2TextureSet, RuntimeTerrainFrameError> {
    match stages {
        [stage] => Ok(M2TextureSet::One(*stage)),
        [first, second] => Ok(M2TextureSet::Two([*first, *second])),
        _ => Err(RuntimeTerrainFrameError::M2TextureStageCount {
            model: model.path().clone(),
            draw_index,
            stage_count: stages.len(),
        }),
    }
}

/// Selects the single BLP slot consumed by the ordinary particle shader.
pub(in super::super) fn ordinary_particle_texture_index(
    model: &DecodedM2Model,
    particle_index: usize,
    emitter: &M2ParticleEmitter,
) -> Result<u16, RuntimeTerrainFrameError> {
    let mut selected = None;
    let mut texture_count = 0;
    for texture_index in emitter.texture_indices().into_iter().flatten() {
        selected = Some(texture_index);
        texture_count += 1;
    }
    if texture_count != 1 {
        return Err(RuntimeTerrainFrameError::M2ParticleTextureCount {
            model: model.path().clone(),
            particle_index,
            texture_count,
        });
    }
    selected.ok_or_else(|| RuntimeTerrainFrameError::M2ParticleTextureCount {
        model: model.path().clone(),
        particle_index,
        texture_count,
    })
}
