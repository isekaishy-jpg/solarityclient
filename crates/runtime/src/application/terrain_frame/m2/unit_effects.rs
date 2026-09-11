//! Named CEffect resources and the sequence callback that retires each model.

#[cfg(test)]
#[path = "../../../../tests/application/unit_effect_lifecycle.rs"]
mod tests;

use crate::application::unit_animation::UnitAnimationBehavior;
use glam::{Mat4, Vec3};
use solarity_asset::{
    AssetStore, BlpTextureCache, EnvironmentalDamageCatalog, M2ModelAnimationMode, M2ModelCache,
    SpellVisualEffectCatalog, SpellVisualEffectDefinition,
};
use solarity_ecs::WorldObjectIdentity;
use solarity_rendering::{M2ModelOrientation, M2SequenceStartPhase};
use solarity_systems::UnitEffectScale;
use solarity_systems::UnitWaterEffect;
use std::collections::{BTreeSet, HashMap};
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::application) enum UnitEffectResource {
    Water(UnitWaterEffect),
    Visual(u32),
}

impl From<UnitWaterEffect> for UnitEffectResource {
    fn from(kind: UnitWaterEffect) -> Self {
        Self::Water(kind)
    }
}

/// Worker-owned declarations, textures, mesh plans, and compiled programs.
pub(in crate::application) struct ResidentUnitEffect {
    kind: UnitEffectResource,
    definition: SpellVisualEffectDefinition,
    source: ResidentM2Source,
}

impl ResidentUnitEffect {
    pub(in crate::application) fn load(
        store: &mut AssetStore,
        environmental: &EnvironmentalDamageCatalog,
    ) -> Result<Vec<Self>, RuntimeTerrainError> {
        let catalog = SpellVisualEffectCatalog::load(store)?;
        let mut models = M2ModelCache::new();
        let mut textures = BlpTextureCache::new();
        let mut effects = Vec::with_capacity(WATER_EFFECTS.len());
        let visuals = environmental
            .visual_kits()
            .flat_map(|kit| kit.effects().map(|(_, id)| id))
            .collect::<BTreeSet<_>>();
        for kind in WATER_EFFECTS
            .into_iter()
            .map(UnitEffectResource::Water)
            .chain(visuals.into_iter().map(UnitEffectResource::Visual))
        {
            let Some(definition) = (match kind {
                UnitEffectResource::Water(water) => catalog.named(water.name()),
                UnitEffectResource::Visual(id) => catalog.definition(id),
            }) else {
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
                    tracing::warn!(effect = definition.id(), %path, %error, "unit effect model request failed");
                }
            }
        }
        Ok(effects)
    }
}

/// Prewarmed immutable GPU generations survive changes to the world frame.
#[derive(Default)]
pub(in crate::application) struct M2UnitEffectSources {
    entries: HashMap<UnitEffectResource, M2UnitEffectSource>,
}

struct M2UnitEffectSource {
    definition: SpellVisualEffectDefinition,
    gpu: M2GpuSource,
}

/// The callback retains its unit generation and the native placement inputs.
pub(in crate::application) struct UnitEffectRequest {
    pub identity: WorldObjectIdentity,
    pub lifetime: Weak<()>,
    pub kind: UnitEffectResource,
    pub kit: Option<u32>,
    pub sound_entry: u32,
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
    kind: UnitEffectResource,
    definition_id: u32,
    kit: Option<u32>,
    sound_entry: u32,
    sound_lifetime: Option<Rc<super::sound::M2SoundKind>>,
    binding: UnitEffectBinding,
    scale: UnitEffectScale,
    phase: UnitEffectPhase,
}

impl UnitEffectPlacement {
    pub(super) fn take_ready_sound(&mut self, position: Vec3) -> Option<super::RuntimeM2Event> {
        if self.retiring() {
            self.sound_lifetime = None;
            return None;
        }
        let entry = std::mem::take(&mut self.sound_entry);
        (entry != 0)
            .then(|| self.sound_event(entry, position))
            .flatten()
            .map(super::RuntimeM2Event::with_effect_kit_sound)
    }

    fn sound_event(&self, entry: u32, position: Vec3) -> Option<super::RuntimeM2Event> {
        Some(
            super::RuntimeM2Event::new(*b"$SND", entry, position, Some(self.identity.guid()))
                .with_sound_owner(super::sound::M2SoundOwner::new(
                    self.sound_lifetime.as_ref()?,
                )),
        )
    }

    pub(super) fn bind_sound_events(&self, events: &mut [super::RuntimeM2Event]) {
        for event in events {
            if event.identifier() == *b"$SND"
                && let Some(sound) = self.sound_event(event.data(), event.position())
            {
                *event = sound;
            }
        }
    }
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
        let result = self.phase.advance(playback, model, now, random);
        if self.retiring() {
            self.sound_lifetime = None;
        }
        result
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
    anchors: HashMap<usize, HashMap<u32, Option<Mat4>>>,
}

impl M2UnitEffectScene {
    /// Retires completed effects only after their particles drain, keeping all
    /// surviving placements in order. The prefix before `first_effect` must
    /// contain no effects; callers pass zero while topology metadata is dirty.
    pub(super) fn retire_drained(
        &mut self,
        placements: &mut Vec<M2GpuPlacement>,
        first_effect: usize,
    ) -> bool {
        let previous = placements.len();
        placements
            .extract_if(first_effect.., |placement| {
                placement.unit_effect.as_ref().is_some_and(|effect| {
                    effect.retiring()
                        && placement
                            .particles
                            .iter()
                            .all(|particle| particle.simulation.particles().is_empty())
                })
            })
            .for_each(drop);
        let changed = previous != placements.len();
        if changed {
            let effects = &placements[first_effect..];
            self.anchors.retain(|parent, attachments| {
                attachments.retain(|id, _| effects.iter().any(|placement| {
                    placement.unit_effect.as_ref().is_some_and(|effect| {
                        matches!(&effect.binding, UnitEffectBinding::Attached { owner, attachment, .. } if owner.as_ptr().addr() == *parent && *attachment == *id)
                    })
                }));
                !attachments.is_empty()
            });
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
            .and_then(|sources| sources.entries.get(&request.kind))
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
            UnitEffectBinding::Attached {
                owner, attachment, ..
            } => {
                self.anchors
                    .entry(owner.as_ptr().addr())
                    .or_default()
                    .entry(*attachment)
                    .or_insert(None);
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
            definition_id: source.definition.id(),
            kit: request.kit,
            sound_entry: request.sound_entry,
            sound_lifetime: Some(Rc::new(super::sound::M2SoundKind::UnitEffect)),
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
        for attachments in self.anchors.values_mut() {
            attachments.values_mut().for_each(|anchor| *anchor = None);
        }
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

    /// Owners with attached effects need a final palette before draw admission.
    pub(super) fn has_anchors(&self, owner: &Rc<UnitAnimationBehavior>) -> bool {
        self.anchors.contains_key(&Rc::as_ptr(owner).addr())
    }

    /// Each requested attachment inherits its current parent bone and placement.
    pub(super) fn update_anchor(
        &mut self,
        owner: &Rc<UnitAnimationBehavior>,
        model: &solarity_asset::DecodedM2Model,
        bones: &solarity_rendering::M2BonePose,
        transform: Mat4,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(attachments) = self.anchors.get_mut(&Rc::as_ptr(owner).addr()) else {
            return Ok(());
        };
        for (id, anchor) in attachments {
            if let Some(attachment) = model.attachment(*id) {
                let bone = bones
                    .transforms()
                    .get(usize::from(attachment.bone_index()))
                    .ok_or(solarity_rendering::M2BonePoseError::AttachmentBoneIndex {
                        requested: attachment.bone_index(),
                        available: bones.transforms().len(),
                    })?;
                *anchor = Some(transform * *bone * Mat4::from_translation(attachment.position()));
            }
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
            owner,
            model_scale,
            attachment,
        } = &effect.binding
        else {
            return;
        };
        let anchor = self
            .anchors
            .get(&owner.as_ptr().addr())
            .and_then(|attachments| attachments.get(attachment))
            .copied()
            .flatten();
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
                        && old.definition_id == new.definition_id
                        && (old.kit.is_none() || old.kit == new.kit)
                        && matches!((&old.binding, &new.binding), (UnitEffectBinding::Attached { attachment: a, .. }, UnitEffectBinding::Attached { attachment: b, .. }) if a == b)
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
            self.sources.entries.insert(
                effect.kind,
                M2UnitEffectSource {
                    definition: effect.definition.clone(),
                    gpu,
                },
            );
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
