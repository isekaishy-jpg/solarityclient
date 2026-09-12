//! Shared immutable mesh preparation and incremental driver pipeline admission.

use super::{RuntimeTerrainFrameError, STOCK_HIGH_CAPABILITY_PROFILE};
use solarity_asset::{AssetPath, DecodedM2Model};
use solarity_rendering::{
    M2LocalLightCount, M2MaterialState, M2MeshPlan, M2ModelOrientation, M2ParticleSpirvCompiler,
    M2ParticleSpirvProgram, M2RibbonSpirvCompiler, M2RibbonSpirvProgram, M2ShaderPermutation,
    M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation, M2SpirvCompiler, M2SpirvKey,
    M2SpirvProgram, VulkanRenderer,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// CPU-only M2 generation prepared away from the presentation thread.
pub(in crate::application) struct M2CpuSource {
    _model: Arc<DecodedM2Model>,
    pub(super) plan: Arc<M2MeshPlan>,
    pub(super) mesh_programs: HashMap<M2SpirvKey, M2SpirvProgram>,
    pub(super) particle_programs: HashMap<M2MaterialState, M2ParticleSpirvProgram>,
    pub(super) ribbon_programs: HashMap<M2MaterialState, M2RibbonSpirvProgram>,
}

/// Driver pipeline work for one worker-prepared Glue M2 source.
///
/// SPIR-V is selected from build-generated modules. Keeping a cursor over the remaining
/// Vulkan programs lets the presentation owner admit one potentially costly
/// program and its mesh shadow partner per frame instead of one 20-30 ms burst when a character
/// with several equipped models first becomes visible.
pub(in crate::application) struct M2GluePipelineWarmup {
    programs: VecDeque<M2GluePipelineProgram>,
}

enum M2GluePipelineProgram {
    Mesh(M2SpirvProgram, M2ModelOrientation),
    Particle(M2ParticleSpirvProgram),
    Ribbon(M2RibbonSpirvProgram),
}

impl M2GluePipelineWarmup {
    /// Cached programs consume no cold-admission slot across source boundaries.
    pub(in crate::application) fn is_resident(&self, renderer: &VulkanRenderer) -> bool {
        self.programs.iter().all(|program| match program {
            M2GluePipelineProgram::Mesh(program, orientation) => {
                renderer.has_precompiled_m2_pipeline(program, *orientation)
            }
            M2GluePipelineProgram::Particle(program) => {
                renderer.has_precompiled_m2_particle_pipeline(program)
            }
            M2GluePipelineProgram::Ribbon(program) => {
                renderer.has_precompiled_m2_ribbon_pipeline(program)
            }
        })
    }

    /// Captures immutable programs for one exact model orientation.
    pub(in crate::application) fn new(
        source: &M2CpuSource,
        orientation: M2ModelOrientation,
    ) -> Self {
        let mut programs = VecDeque::with_capacity(
            source.mesh_programs.len()
                + source.particle_programs.len()
                + source.ribbon_programs.len(),
        );
        programs.extend(
            source
                .mesh_programs
                .values()
                .cloned()
                .map(|program| M2GluePipelineProgram::Mesh(program, orientation)),
        );
        programs.extend(
            source
                .particle_programs
                .values()
                .cloned()
                .map(M2GluePipelineProgram::Particle),
        );
        programs.extend(
            source
                .ribbon_programs
                .values()
                .cloned()
                .map(M2GluePipelineProgram::Ribbon),
        );
        Self { programs }
    }

    /// Admits one cold program (mesh plus shadow partner), skipping already-resident identities.
    pub(in crate::application) fn service_one(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        while let Some(program) = self.programs.pop_front() {
            let resident = match &program {
                M2GluePipelineProgram::Mesh(program, orientation) => {
                    renderer.has_precompiled_m2_pipeline(program, *orientation)
                }
                M2GluePipelineProgram::Particle(program) => {
                    renderer.has_precompiled_m2_particle_pipeline(program)
                }
                M2GluePipelineProgram::Ribbon(program) => {
                    renderer.has_precompiled_m2_ribbon_pipeline(program)
                }
            };
            if resident {
                continue;
            }
            match program {
                M2GluePipelineProgram::Mesh(program, orientation) => {
                    renderer.prepare_precompiled_oriented_m2_pipeline(&program, orientation)?;
                }
                M2GluePipelineProgram::Particle(program) => {
                    renderer.prepare_precompiled_m2_particle_pipeline(&program)?;
                }
                M2GluePipelineProgram::Ribbon(program) => {
                    renderer.prepare_precompiled_m2_ribbon_pipeline(&program)?;
                }
            }
            break;
        }
        Ok(self.programs.is_empty())
    }
}

/// Exact immutable CPU generation shared by Glue character placements.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::application) struct M2GlueCpuSourceKey {
    path: AssetPath,
    local_light_count: M2LocalLightCount,
}

impl M2GlueCpuSourceKey {
    /// Identifies one model and stock local-light shader permutation.
    pub(in crate::application) fn new(
        path: AssetPath,
        local_light_count: M2LocalLightCount,
    ) -> Self {
        Self {
            path,
            local_light_count,
        }
    }

    /// Returns the exact shader light count represented by this key.
    pub(in crate::application) const fn local_light_count(&self) -> M2LocalLightCount {
        self.local_light_count
    }

    /// Returns the canonical archive model path represented by this key.
    pub(in crate::application) const fn path(&self) -> &AssetPath {
        &self.path
    }
}

/// Process-wide worker bytecode indexed by complete stock shader identity.
#[derive(Default)]
struct M2GlueProgramCache {
    mesh: HashMap<M2SpirvKey, M2SpirvProgram>,
    particles: HashMap<M2MaterialState, M2ParticleSpirvProgram>,
    ribbons: HashMap<M2MaterialState, M2RibbonSpirvProgram>,
}

/// Shares immutable worker results across every finite Glue backdrop.
static M2_PROGRAMS: OnceLock<Mutex<M2GlueProgramCache>> = OnceLock::new();

/// Weak entries retain neither decoded assets nor meshes after their last owner.
type M2SourceCache = HashMap<(usize, M2LocalLightCount), Weak<M2CpuSource>>;
static M2_SOURCES: OnceLock<Mutex<M2SourceCache>> = OnceLock::new();

/// Builds or shares the immutable mesh plan and required prebuilt shader selections.
pub(in crate::application) fn prepare_m2_cpu_source(
    model: &Arc<DecodedM2Model>,
    local_light_count: M2LocalLightCount,
) -> Result<Arc<M2CpuSource>, RuntimeTerrainFrameError> {
    let key = (Arc::as_ptr(model) as usize, local_light_count);
    let sources = M2_SOURCES.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let mut sources = sources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(source) = sources.get(&key).and_then(Weak::upgrade) {
            return Ok(source);
        }
        sources.retain(|_, source| source.strong_count() != 0);
    }
    let plan = Arc::new(M2MeshPlan::prepare(model, STOCK_HIGH_CAPABILITY_PROFILE)?);
    let cache_lock = M2_PROGRAMS.get_or_init(|| Mutex::new(M2GlueProgramCache::default()));
    let mut mesh_compiler = None;
    let mut mesh_programs = HashMap::new();
    for draw in plan.draws() {
        let shader = M2ShaderPlan::resolve(model, draw)?;
        let permutation = M2ShaderPermutation::resolve(
            draw,
            local_light_count,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        );
        let material = M2MaterialState::from_material(draw.material());
        let shaders = [
            Some(shader),
            (!material.blend_enabled()).then(|| shader.with_runtime_alpha_fade()),
        ];
        for shader in shaders.into_iter().flatten() {
            let key = M2SpirvKey::new(shader, permutation);
            if let std::collections::hash_map::Entry::Vacant(entry) = mesh_programs.entry(key) {
                let cached = match cache_lock.lock() {
                    Ok(cache) => cache.mesh.get(&key).cloned(),
                    Err(poisoned) => poisoned.into_inner().mesh.get(&key).cloned(),
                };
                let program = if let Some(program) = cached {
                    program
                } else {
                    let compiler = match mesh_compiler.as_ref() {
                        Some(compiler) => compiler,
                        None => mesh_compiler.insert(M2SpirvCompiler::new()?),
                    };
                    let program = compiler.compile(shader, permutation)?;
                    match cache_lock.lock() {
                        Ok(mut cache) => cache.mesh.entry(key).or_insert(program).clone(),
                        Err(poisoned) => poisoned
                            .into_inner()
                            .mesh
                            .entry(key)
                            .or_insert(program)
                            .clone(),
                    }
                };
                entry.insert(program);
            }
        }
    }

    let mut particle_programs = HashMap::new();
    if !model.animations().particles().is_empty() {
        let mut particle_compiler = None;
        for emitter in model.animations().particles() {
            let material = M2MaterialState::from_particle(emitter.blending_type(), emitter.flags());
            for material in std::iter::once(material)
                .chain((!material.blend_enabled()).then(|| material.with_runtime_alpha_fade()))
            {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    particle_programs.entry(material)
                {
                    let cached = match cache_lock.lock() {
                        Ok(cache) => cache.particles.get(&material).cloned(),
                        Err(poisoned) => poisoned.into_inner().particles.get(&material).cloned(),
                    };
                    let program = if let Some(program) = cached {
                        program
                    } else {
                        let compiler = match particle_compiler.as_ref() {
                            Some(compiler) => compiler,
                            None => particle_compiler.insert(M2ParticleSpirvCompiler::new()?),
                        };
                        let program = compiler.compile(material)?;
                        match cache_lock.lock() {
                            Ok(mut cache) => {
                                cache.particles.entry(material).or_insert(program).clone()
                            }
                            Err(poisoned) => poisoned
                                .into_inner()
                                .particles
                                .entry(material)
                                .or_insert(program)
                                .clone(),
                        }
                    };
                    entry.insert(program);
                }
            }
        }
    }

    let mut ribbon_programs = HashMap::new();
    if !model.animations().ribbons().is_empty() {
        let mut ribbon_compiler = None;
        for emitter in model.animations().ribbons() {
            for material_index in emitter.material_indices() {
                let material =
                    M2MaterialState::from_material(model.materials()[usize::from(*material_index)]);
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    ribbon_programs.entry(material)
                {
                    let cached = match cache_lock.lock() {
                        Ok(cache) => cache.ribbons.get(&material).cloned(),
                        Err(poisoned) => poisoned.into_inner().ribbons.get(&material).cloned(),
                    };
                    let program = if let Some(program) = cached {
                        program
                    } else {
                        let compiler = match ribbon_compiler.as_ref() {
                            Some(compiler) => compiler,
                            None => ribbon_compiler.insert(M2RibbonSpirvCompiler::new()?),
                        };
                        let program = compiler.compile(material)?;
                        match cache_lock.lock() {
                            Ok(mut cache) => {
                                cache.ribbons.entry(material).or_insert(program).clone()
                            }
                            Err(poisoned) => poisoned
                                .into_inner()
                                .ribbons
                                .entry(material)
                                .or_insert(program)
                                .clone(),
                        }
                    };
                    entry.insert(program);
                }
            }
        }
    }
    let source = Arc::new(M2CpuSource {
        _model: Arc::clone(model),
        plan,
        mesh_programs,
        particle_programs,
        ribbon_programs,
    });
    // Concurrent misses may prepare independently; publication chooses one
    // immutable source. Its model pin makes allocation-address reuse impossible.
    let mut sources = sources
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = sources.get(&key).and_then(Weak::upgrade) {
        return Ok(existing);
    }
    sources.insert(key, Arc::downgrade(&source));
    Ok(source)
}
