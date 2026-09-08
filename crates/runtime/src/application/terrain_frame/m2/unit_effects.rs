//! Named CEffect resources and the sequence callback that retires each model.

#[cfg(test)]
#[path = "../../../../tests/application/unit_effect_lifecycle.rs"]
mod tests;

use crate::application::unit_animation::UnitAnimationBehavior;
use glam::{Mat4, Vec3};
use solarity_asset::{
    AssetStore, BlpTextureCache, M2ModelAnimationMode, M2ModelCache, SpellVisualEffectCatalog,
    SpellVisualEffectDefinition,
};
use solarity_ecs::WorldObjectIdentity;
use solarity_rendering::{M2ModelOrientation, M2SequenceStartPhase};
use solarity_systems::UnitEffectScale;
use solarity_systems::UnitWaterEffect;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use super::{
    M2GluePipelineWarmup, M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, M2Playback,
    M2PlaybackAdvance, M2PlaybackStorage, ResidentM2Source, RuntimeTerrainFrameError,
    VulkanRenderer, m2_gpu_placement, prepare_source,
};
use crate::application::RuntimeTerrainError;
use crate::random::CrtRand;

pub(in crate::application) const WATER_EFFECTS: [UnitWaterEffect; 5] = [
    UnitWaterEffect::RunSpray,
    UnitWaterEffect::WalkSpray,
    UnitWaterEffect::UnderwaterBreath,
    UnitWaterEffect::ColdBreath,
    UnitWaterEffect::InebriatedBubbles,
];

/// Worker-owned declarations, textures, mesh plans, and compiled programs.
pub(in crate::application) struct ResidentUnitEffect {
    kind: UnitWaterEffect,
    definition: SpellVisualEffectDefinition,
    source: ResidentM2Source,
}

impl ResidentUnitEffect {
    pub(in crate::application) fn load(
        store: &mut AssetStore,
    ) -> Result<Vec<Self>, RuntimeTerrainError> {
        let catalog = SpellVisualEffectCatalog::load(store)?;
        let mut models = M2ModelCache::new();
        let mut textures = BlpTextureCache::new();
        let mut effects = Vec::with_capacity(WATER_EFFECTS.len());
        for kind in WATER_EFFECTS {
            let Some(definition) = catalog.named(kind.name()) else {
                continue;
            };
            let Some(path) = definition.model_path()? else {
                continue;
            };
            match ResidentM2Source::load(&path, &mut models, &mut textures, store) {
                Ok(source) => effects.push(Self {
                    kind,
                    definition: definition.clone(),
                    source,
                }),
                Err(error) => {
                    tracing::warn!(effect = kind.name(), %path, %error, "unit effect model request failed");
                }
            }
        }
        Ok(effects)
    }
}

/// Prewarmed immutable GPU generations survive changes to the world frame.
#[derive(Default)]
pub(in crate::application) struct M2UnitEffectSources {
    entries: [Option<M2UnitEffectSource>; 8],
}

struct M2UnitEffectSource {
    definition: SpellVisualEffectDefinition,
    gpu: M2GpuSource,
}

/// The callback retains its unit generation and the native placement inputs.
pub(in crate::application) struct UnitEffectRequest {
    pub identity: WorldObjectIdentity,
    pub lifetime: Weak<()>,
    pub kind: UnitWaterEffect,
    pub binding: UnitEffectBinding,
}

pub(in crate::application) type UnitEffectEventCallback<'a> = dyn FnMut(
        &super::RuntimeM2Event,
        &Rc<UnitAnimationBehavior>,
        &solarity_asset::DecodedM2Model,
        Mat4,
    ) -> Option<UnitEffectRequest>
    + 'a;

pub(in crate::application) enum UnitEffectBinding {
    Positioned {
        position: Vec3,
        world_factor: f32,
        unit_scale: f32,
    },
    Attached {
        owner: Weak<UnitAnimationBehavior>,
        model_scale: f32,
        attachment: u32,
    },
}

pub(super) struct UnitEffectPlacement {
    identity: WorldObjectIdentity,
    lifetime: Weak<()>,
    kind: UnitWaterEffect,
    binding: UnitEffectBinding,
    scale: UnitEffectScale,
    phase: UnitEffectPhase,
}

impl UnitEffectPlacement {
    pub(super) fn attached_to(&self, owner: &Rc<UnitAnimationBehavior>) -> bool {
        matches!(&self.binding, UnitEffectBinding::Attached { owner: parent, .. } if std::ptr::eq(parent.as_ptr(), Rc::as_ptr(owner)))
    }

    pub(super) fn retiring(&self) -> bool {
        self.phase == UnitEffectPhase::Retiring
    }

    pub(super) fn advance(
        &mut self,
        playback: &mut M2Playback,
        model: &solarity_asset::DecodedM2Model,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        self.phase.advance(playback, model, now, random)
    }
}

struct PendingUnitEffect {
    source: M2GpuSource,
    placement: M2GpuPlacement,
}

#[derive(Default)]
pub(super) struct M2UnitEffectScene {
    pub(super) sources: Option<Arc<M2UnitEffectSources>>,
    next_serial: u64,
    pending: std::collections::VecDeque<PendingUnitEffect>,
    loading: std::collections::VecDeque<(UnitEffectRequest, u32)>,
    anchors: std::collections::HashMap<usize, Option<Mat4>>,
}

impl M2UnitEffectScene {
    pub(super) fn retire_drained(&mut self, placements: &mut Vec<M2GpuPlacement>) -> bool {
        let previous = placements.len();
        placements.retain(|placement| {
            placement.unit_effect.as_ref().is_none_or(|effect| {
                !effect.retiring()
                    || placement
                        .particles
                        .iter()
                        .any(|particle| !particle.simulation.particles().is_empty())
            })
        });
        let changed = previous != placements.len();
        if changed {
            self.anchors.retain(|key, _| placements.iter().any(|placement| {
                placement.unit_effect.as_ref().is_some_and(|effect| {
                    matches!(&effect.binding, UnitEffectBinding::Attached { owner, .. } if owner.as_ptr().addr() == *key)
                })
            }));
        }
        changed
    }
    /// Model construction consumes its default-sequence rolls at callback
    /// dispatch, before another unit can advance the shared CRT stream.
    pub(super) fn emit(
        &mut self,
        request: UnitEffectRequest,
        animations: &solarity_asset::AnimationDataCatalog,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.sources.is_none() {
            self.loading.push_back((request, now as u32));
            return Ok(());
        }
        self.construct(
            request,
            animations,
            now as u32,
            now as u32,
            M2SequenceStartPhase::DuringSceneUpdate,
            random,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn construct(
        &mut self,
        request: UnitEffectRequest,
        animations: &solarity_asset::AnimationDataCatalog,
        created: u32,
        now: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(source) = self
            .sources
            .as_ref()
            .and_then(|sources| sources.entries[request.kind as usize].as_ref())
        else {
            return Ok(());
        };
        let scale = UnitEffectScale::from(&source.definition);
        let transform = match &request.binding {
            UnitEffectBinding::Positioned {
                position,
                world_factor,
                unit_scale,
            } => {
                Mat4::from_translation(*position)
                    * Mat4::from_scale(Vec3::splat(scale.positioned(*world_factor, *unit_scale)))
            }
            UnitEffectBinding::Attached { owner, .. } => {
                self.anchors.entry(owner.as_ptr().addr()).or_insert(None);
                Mat4::IDENTITY
            }
        };
        let model = &source.gpu.model;
        let playback = M2Playback::unit_effect_default_sequence(
            model, animations, created, now, phase, random,
        )?;
        let mut placement = m2_gpu_placement(
            0,
            transform,
            M2GpuPlacementOwner::UnitEffect {
                serial: self.next_serial,
            },
            model,
            Some(M2PlaybackStorage::Local(playback)),
            None,
            created,
        )?;
        self.next_serial = self.next_serial.wrapping_add(1);
        placement.unit_effect = Some(UnitEffectPlacement {
            identity: request.identity,
            lifetime: request.lifetime,
            kind: request.kind,
            binding: request.binding,
            scale,
            phase: UnitEffectPhase::Playing,
        });
        self.pending.push_back(PendingUnitEffect {
            source: source.gpu.clone(),
            placement,
        });
        Ok(())
    }

    pub(super) fn begin_frame(&mut self) {
        self.anchors.values_mut().for_each(|anchor| *anchor = None);
    }

    pub(super) fn publish_loaded(
        &mut self,
        animations: &solarity_asset::AnimationDataCatalog,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.sources.is_some() {
            while let Some((request, created)) = self.loading.pop_front() {
                if request.lifetime.strong_count() == 0 {
                    continue;
                }
                if matches!(&request.binding, UnitEffectBinding::Attached { owner, .. } if owner.strong_count() == 0)
                {
                    continue;
                }
                self.construct(
                    request,
                    animations,
                    created,
                    now as u32,
                    M2SequenceStartPhase::BeforeSceneUpdate,
                    random,
                )?;
            }
        }
        Ok(())
    }

    /// Native mouth selection uses 17, then root attachment 19. Its matrix
    /// inherits the current animated parent bone and the model placement.
    pub(super) fn update_anchor(
        &mut self,
        owner: &Rc<UnitAnimationBehavior>,
        model: &solarity_asset::DecodedM2Model,
        bones: &solarity_rendering::M2BonePose,
        transform: Mat4,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(anchor) = self.anchors.get_mut(&Rc::as_ptr(owner).addr()) else {
            return Ok(());
        };
        if let Some(attachment) = model.attachment(17).or_else(|| model.attachment(19)) {
            let bone = bones
                .transforms()
                .get(usize::from(attachment.bone_index()))
                .ok_or(solarity_rendering::M2BonePoseError::AttachmentBoneIndex {
                    requested: attachment.bone_index(),
                    available: bones.transforms().len(),
                })?;
            *anchor = Some(transform * *bone * Mat4::from_translation(attachment.position()));
        }
        Ok(())
    }

    pub(super) fn prepare_attachment(&self, placement: &mut M2GpuPlacement) {
        let Some(effect) = &mut placement.unit_effect else {
            return;
        };
        // Unit destruction retires every owned CEffect, including positioned
        // spray. The unit lifetime survives ordinary body model replacement.
        if effect.lifetime.strong_count() == 0 {
            effect.phase = UnitEffectPhase::Retiring;
        }
        let UnitEffectBinding::Attached {
            owner, model_scale, ..
        } = &effect.binding
        else {
            return;
        };
        let anchor = self.anchors.get(&owner.as_ptr().addr()).copied().flatten();
        if owner.strong_count() == 0 || anchor.is_none() {
            effect.phase = UnitEffectPhase::Retiring;
        } else if let Some(anchor) = anchor {
            let scale = effect
                .scale
                .attached(*model_scale, anchor.x_axis.truncate().as_dvec3().length());
            placement.transform = anchor * Mat4::from_scale(Vec3::splat(scale));
        }
    }

    /// Append queued models after their parents, retaining shared GPU sources.
    /// Repeated attached requests retire the prior matching CEffect first.
    pub(super) fn publish(
        &mut self,
        placements: &mut Vec<M2GpuPlacement>,
        sources: &mut Vec<Option<M2GpuSource>>,
    ) -> bool {
        let changed = !self.pending.is_empty();
        while let Some(mut pending) = self.pending.pop_front() {
            if let Some(new) = &pending.placement.unit_effect
                && matches!(new.binding, UnitEffectBinding::Attached { .. })
            {
                for old in placements
                    .iter_mut()
                    .filter_map(|placement| placement.unit_effect.as_mut())
                {
                    if old.identity == new.identity
                        && old.kind == new.kind
                        && matches!(&old.binding, UnitEffectBinding::Attached { attachment, .. } if Some(*attachment) == new.kind.attachment())
                    {
                        old.phase = UnitEffectPhase::Retiring;
                    }
                }
            }
            let index = placements
                .iter()
                .find_map(|placement| {
                    placement
                        .unit_effect
                        .as_ref()
                        .filter(|effect| {
                            Some(effect.kind)
                                == pending
                                    .placement
                                    .unit_effect
                                    .as_ref()
                                    .map(|effect| effect.kind)
                        })
                        .map(|_| placement.source_index)
                })
                .unwrap_or_else(|| {
                    sources.push(Some(pending.source));
                    sources.len() - 1
                });
            pending.placement.source_index = index;
            placements.push(pending.placement);
        }
        changed
    }
}

/// Amortizes driver compilation before publishing the complete source bank.
pub(in crate::application) struct M2UnitEffectWarmup {
    pending: std::collections::VecDeque<ResidentUnitEffect>,
    current: Option<M2GluePipelineWarmup>,
    sources: M2UnitEffectSources,
}

impl M2UnitEffectWarmup {
    pub(in crate::application) fn new(effects: Vec<ResidentUnitEffect>) -> Self {
        Self {
            pending: effects.into(),
            current: None,
            sources: M2UnitEffectSources::default(),
        }
    }

    /// Creates at most one driver pipeline per call; source uploads follow its
    /// final pipeline so an authored callback never compiles a shader.
    pub(in crate::application) fn service_one(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(effect) = self.pending.front() else {
            return Ok(true);
        };
        let warmup = self.current.get_or_insert_with(|| {
            M2GluePipelineWarmup::new(effect.source.cpu_source(), M2ModelOrientation::Authored)
        });
        if !warmup.service_one(renderer)? {
            return Ok(false);
        }
        if let Some(gpu) = prepare_source(renderer, &effect.source)? {
            self.sources.entries[effect.kind as usize] = Some(M2UnitEffectSource {
                definition: effect.definition.clone(),
                gpu,
            });
        }
        self.pending.pop_front();
        self.current = None;
        Ok(self.pending.is_empty())
    }

    pub(in crate::application) fn into_sources(self) -> M2UnitEffectSources {
        self.sources
    }
}

/// `744870` installs Despawn when authored; `743580` then retires it. Other
/// completions pin the primary before entering the shared retirement list.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum UnitEffectPhase {
    #[default]
    Playing,
    Despawning,
    Retiring,
}

impl UnitEffectPhase {
    pub(super) fn advance(
        &mut self,
        playback: &mut M2Playback,
        model: &solarity_asset::DecodedM2Model,
        scene_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        if *self == Self::Retiring {
            return playback.clock(model, scene_time_ms, random);
        }
        playback.clock_with_completion(
            model,
            scene_time_ms,
            random,
            Some(&mut |playback, random| {
                if *self == Self::Playing {
                    if model.animations().has_model_animation(159) {
                        *self = Self::Despawning;
                        playback.apply_resolved_model_sequence(
                            model,
                            159,
                            M2ModelAnimationMode::Forward,
                            0,
                            playback.scene_time_ms,
                            M2SequenceStartPhase::DuringSceneUpdate,
                            true,
                            random,
                        )?;
                        return Ok(());
                    }
                    playback.freeze_sequence_end(playback.scene_time_ms);
                }
                *self = Self::Retiring;
                Ok(())
            }),
        )
    }
}
