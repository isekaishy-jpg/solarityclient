//! Unit registration clocks and synchronous authored CEffect requests.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetStore, CreatureCatalog, DecodedM2Model, LiquidTypeCatalog,
};
use solarity_cpu::{CpuExecutor, CpuTask};
use solarity_ecs::{ActiveWorld, ObjectFields, ObjectKind, WorldObjectIdentity};
use solarity_rendering::VulkanRenderer;
use solarity_systems::{
    UnitBreathEnvironment, UnitBreathState, UnitWaterSprayInput,
    resolve_unit_movement_speed_extended, unit_player_inebriation, unit_world_effect_factor,
};

use super::character_directory::RuntimeCharacterMetadata;
use super::terrain_frame::RuntimeM2Event;
use super::terrain_frame::m2::unit_effects::{
    M2UnitEffectSources, M2UnitEffectWarmup, ResidentUnitEffect, UnitEffectBinding,
    UnitEffectRequest, UnitEffectResource,
};
use super::unit_animation::UnitAnimationBehavior;
use super::unit_water::UnitWaterSample;
use super::{
    ApplicationError, RuntimePlayerPresentation, RuntimeTerrainCoordinator, RuntimeTerrainError,
};

enum Sources {
    Deferred(ArchiveCatalog),
    Running(CpuTask<Result<Vec<ResidentUnitEffect>, RuntimeTerrainError>>),
    Warming(Box<M2UnitEffectWarmup>),
    Ready(Arc<M2UnitEffectSources>),
}

#[derive(Default)]
struct UnitState {
    lifetime: Rc<()>,
    model: Weak<DecodedM2Model>,
    sample: Option<UnitWaterSample>,
    position: Vec3,
    surface: Option<f32>,
    breath: UnitBreathState,
    tint: solarity_systems::UnitModelTint,
}

pub(super) struct RuntimeUnitEffects {
    sources: Option<Sources>,
    environmental: Arc<solarity_asset::EnvironmentalDamageCatalog>,
    world: Option<WorldObjectIdentity>,
    units: HashMap<WorldObjectIdentity, UnitState>,
}

impl RuntimeUnitEffects {
    pub(super) fn new(
        catalog: ArchiveCatalog,
        environmental: Arc<solarity_asset::EnvironmentalDamageCatalog>,
    ) -> Self {
        Self {
            sources: Some(Sources::Deferred(catalog)),
            environmental,
            world: None,
            units: HashMap::new(),
        }
    }

    /// Worker decoding and one driver pipeline per service tick precede callbacks.
    pub(super) fn service_sources(
        &mut self,
        cpu: &CpuExecutor,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), ApplicationError> {
        self.sources = Some(
            match self
                .sources
                .take()
                .ok_or(ApplicationError::UnitEffectPreparationFailed)?
            {
                Sources::Deferred(catalog) if cpu.can_admit_speculative()? => {
                    match cpu.try_reserve() {
                        Ok(permit) => {
                            let environmental = Arc::clone(&self.environmental);
                            Sources::Running(permit.submit(move || {
                                ResidentUnitEffect::load(
                                    &mut AssetStore::mount(catalog)?,
                                    &environmental,
                                )
                            }))
                        }
                        Err(solarity_cpu::CpuError::AtCapacity { .. }) => {
                            Sources::Deferred(catalog)
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Sources::Running(task) if task.is_finished() => {
                    Sources::Warming(Box::new(M2UnitEffectWarmup::new(task.join()??)))
                }
                Sources::Warming(mut warmup) => {
                    if warmup.service_one(renderer)? {
                        Sources::Ready(Arc::new(warmup.into_sources()))
                    } else {
                        Sources::Warming(warmup)
                    }
                }
                state => state,
            },
        );
        Ok(())
    }

    pub(super) fn sources(&self) -> Option<Arc<M2UnitEffectSources>> {
        match &self.sources {
            Some(Sources::Ready(sources)) => Some(Arc::clone(sources)),
            _ => None,
        }
    }

    pub(super) fn synchronize_world(&mut self, world: Option<&ActiveWorld>) {
        let identity = world.and_then(|world| {
            world
                .local_player_guid()
                .ok()
                .and_then(|guid| world.object_identity(guid))
        });
        if self.world != identity {
            self.world = identity;
            self.units.clear();
        }
        self.units.retain(|identity, _| {
            world.is_some_and(|world| world.object_identity(identity.guid()) == Some(*identity))
        });
    }

    pub(super) fn record_sample(&mut self, sample: UnitWaterSample) {
        let state = self.units.entry(sample.identity).or_default();
        state.position = sample.transform.position();
        state.surface = sample.liquid.map(|liquid| liquid.surface_height);
        state.sample = Some(sample);
    }

    /// 73DAB0 refreshes from the previous registration, before movement updates it.
    pub(super) fn refresh_breaths(
        &mut self,
        world: Option<&ActiveWorld>,
        terrain: &mut RuntimeTerrainCoordinator,
        metadata: &RuntimeCharacterMetadata,
        now: u32,
    ) -> Result<(), ApplicationError> {
        let Some(world) = world else {
            return Ok(());
        };
        for (identity, state) in &mut self.units {
            if state.breath.refresh_due(now)
                && let Some(model) = state.model.upgrade()
            {
                refresh(state, &model, *identity, world, terrain, metadata, now)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn synchronize_models(
        &mut self,
        world: Option<&ActiveWorld>,
        player: &RuntimePlayerPresentation,
        terrain: &mut RuntimeTerrainCoordinator,
        metadata: &RuntimeCharacterMetadata,
        liquids: &LiquidTypeCatalog,
        now: u32,
    ) -> Result<(), ApplicationError> {
        let Some(world) = world else {
            return Ok(());
        };
        for (identity, model) in player.unit_effect_models() {
            let state = self.units.entry(identity).or_default();
            if let Some(owner) = player.unit_effect_owner(identity) {
                owner.set_model_color(state.tint.sample(now, u32::MAX));
            }
            if !std::ptr::eq(state.model.as_ptr(), Arc::as_ptr(model)) {
                if state.sample.is_none()
                    && let Some(transform) = world.object_transform(identity.guid())
                {
                    state.position = transform.position();
                    state.surface = terrain
                        .unit_submerged_liquid(state.position, liquids)?
                        .map(|liquid| liquid.surface_height);
                }
                state.model = Arc::downgrade(model);
                refresh(state, model, identity, world, terrain, metadata, now)?;
            }
        }
        Ok(())
    }

    pub(super) fn environmental_impact(
        &mut self,
        impact: &super::gameplay_coordinator::environmental_damage::RuntimeEnvironmentalDamageSnapshot,
        world: &ActiveWorld,
        player: &RuntimePlayerPresentation,
        creatures: &CreatureCatalog,
    ) -> Vec<UnitEffectRequest> {
        if world.object_identity(impact.identity.guid()) != Some(impact.identity) {
            return Vec::new();
        }
        let Some(kit) = self.environmental.visual_kit(impact.packet.kind) else {
            return Vec::new();
        };
        let state = self.units.entry(impact.identity).or_default();
        for (kind, parameters) in kit.special_effects() {
            if kind == 13 {
                state.tint.apply(impact.timestamp_ms, parameters);
            }
        }
        let Some(owner) = player.unit_effect_owner(impact.identity) else {
            return Vec::new();
        };
        owner.set_model_color(state.tint.sample(impact.timestamp_ms, u32::MAX));
        if let Ok(animation) = u16::try_from(kit.animation()) {
            owner.request_environmental_animation(
                animation,
                impact.attack_target_guid,
                impact.template_flags,
            );
        }
        let definition = world
            .unit_presentation(impact.identity.guid())
            .and_then(|unit| creatures.display(unit.display_id()))
            .and_then(|display| creatures.model(display.model_id()));
        kit.effects()
            .enumerate()
            .filter_map(|(index, (attachment, id))| {
                let attachment = u32::try_from(attachment).ok()?;
                Some(UnitEffectRequest {
                    identity: impact.identity,
                    lifetime: Rc::downgrade(&state.lifetime),
                    kind: UnitEffectResource::Visual(id),
                    kit: Some(kit.id()),
                    sound_entry: if index == 0 { kit.sound_entry_id() } else { 0 },
                    binding: UnitEffectBinding::Attached {
                        owner: Rc::downgrade(owner),
                        model_scale: definition.map_or(1.0, |model| model.attached_effect_scale()),
                        attachment,
                    },
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn request(
        &self,
        event: &RuntimeM2Event,
        owner: &Rc<UnitAnimationBehavior>,
        model: &DecodedM2Model,
        transform: Mat4,
        world: &ActiveWorld,
        creatures: &CreatureCatalog,
        camera: Vec3,
        enabled: bool,
    ) -> Option<UnitEffectRequest> {
        let marker = event.identifier();
        if marker != *b"$BTH" && !foot_contact(marker) {
            return None;
        }
        let guid = event.owner_guid()?;
        let identity = world.object_identity(guid)?;
        let state = self.units.get(&identity)?;
        let presentation = world.unit_presentation(guid)?;
        let definition = creatures
            .display(presentation.display_id())
            .and_then(|display| creatures.model(display.model_id()));
        let fields = world
            .entity_by_guid(guid)
            .and_then(|entity| world.storage().get::<&ObjectFields>(entity).ok())?;
        let player = world.object_kind(guid) == Some(ObjectKind::Player);
        let (kind, binding) = if marker == *b"$BTH" {
            let kind = state.breath.effect(
                0,
                definition.map(|model| model.flags()),
                player.then(|| {
                    unit_player_inebriation((fields.get(155) >> 8) as u8, fields.get(322))
                }),
            )?;
            let attachment = if model.attachment(17).is_some() {
                17
            } else {
                19
            };
            (
                kind,
                UnitEffectBinding::Attached {
                    owner: Rc::downgrade(owner),
                    model_scale: definition.map_or(1., |model| model.attached_effect_scale()),
                    attachment,
                },
            )
        } else {
            let sample = state.sample?;
            let liquid = sample.liquid?;
            let (kind, position) = UnitWaterSprayInput {
                foot: event.position(),
                camera,
                origin_z: sample.transform.position().z,
                model_height: sample.height,
                liquid_id: liquid.liquid_type,
                liquid_surface: liquid.surface_height,
                movement_flags: sample.movement.flags() as u32,
                mount_display_id: presentation.mount_display_id(),
                unit_bytes1: fields.get(74),
                player_flags: player.then(|| fields.get(150)),
                transport_guid: sample.movement.transport_guid().unwrap_or(0),
                model_flags: definition?.flags(),
                enabled,
                speed: resolve_unit_movement_speed_extended(sample.movement),
                walk_speed: sample.movement.speeds().walk(),
            }
            .resolve()?;
            let bounds = model.bounds();
            (
                kind,
                UnitEffectBinding::Positioned {
                    position,
                    world_factor: unit_world_effect_factor(
                        bounds.minimum(),
                        bounds.maximum(),
                        definition?.world_effect_scale(),
                    ),
                    unit_scale: transform.x_axis.truncate().length(),
                },
            )
        };
        Some(UnitEffectRequest {
            identity,
            lifetime: Rc::downgrade(&state.lifetime),
            kind: kind.into(),
            kit: None,
            sound_entry: 0,
            binding,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    state: &mut UnitState,
    model: &DecodedM2Model,
    identity: WorldObjectIdentity,
    world: &ActiveWorld,
    terrain: &mut RuntimeTerrainCoordinator,
    metadata: &RuntimeCharacterMetadata,
    now: u32,
) -> Result<(), ApplicationError> {
    let location = terrain.unit_world_model_location(state.position)?;
    state.breath.refresh(
        now,
        UnitBreathEnvironment {
            model_height: model.bounds().maximum().z - model.bounds().minimum().z,
            unit_scale: world
                .object_presentation(identity.guid())
                .map_or(1., |presentation| presentation.scale()),
            origin_z: state.position.z,
            liquid_surface: state.surface,
            cold_area: metadata.cold_area_at(terrain.area_id_at(state.position), location),
        },
    );
    Ok(())
}

/// 732650 dispatches all forty left/right contact variants to 723A50.
fn foot_contact(marker: [u8; 4]) -> bool {
    matches!(
        marker,
        [
            b'$',
            b'B' | b'F' | b'R' | b'S' | b'W',
            b'L' | b'R',
            b'0'..=b'3'
        ]
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn contact_markers_match_original_unit_dispatch() -> Result<(), Box<dyn std::error::Error>> {
        let mut cases = 0;
        let mut contacts = 0;
        for line in include_str!("../../tests/fixtures/unit_effect_contacts.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let mut fields = line.split_whitespace();
            let hex = fields.next().ok_or("marker")?;
            let mut marker = [0; 4];
            for (i, byte) in marker.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
            }
            let left = fields.next().ok_or("handler")?.parse::<i32>()?;
            assert_eq!(super::foot_contact(marker), left >= 0, "{marker:?}");
            if left >= 0 {
                assert_eq!(i32::from(marker[2] == b'L'), left);
                contacts += 1;
            }
            cases += 1;
        }
        assert_eq!((cases, contacts), (185, 40));
        Ok(())
    }
}
