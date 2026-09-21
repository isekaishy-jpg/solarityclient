//! Ordered world registration drives breath state independently of asset loading.

use super::{RuntimeUnitEffects, UnitState};
use crate::application::character_directory::RuntimeCharacterMetadata;
use crate::application::unit_water::UnitWaterSample;
use crate::application::{ApplicationError, RuntimePlayerPresentation, RuntimeTerrainCoordinator};
use solarity_asset::{DecodedM2Model, LiquidTypeCatalog, ResourceLease};
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_systems::UnitBreathEnvironment;

impl RuntimeUnitEffects {
    pub(in crate::application) fn synchronize_world(&mut self, world: Option<&ActiveWorld>) {
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

    pub(in crate::application) fn record_sample(&mut self, sample: UnitWaterSample) {
        let state = self.units.entry(sample.identity).or_default();
        state.position = sample.transform.position();
        state.surface = sample.liquid.map(|liquid| liquid.surface_height);
        state.sample = Some(sample);
    }

    /// 73DAB0 refreshes from the previous registration, before movement updates it.
    pub(in crate::application) fn refresh_breaths(
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
    pub(in crate::application) fn synchronize_models(
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
            if !std::ptr::eq(state.model.as_ptr(), ResourceLease::as_ptr(model)) {
                if state.sample.is_none()
                    && let Some(transform) = world.object_transform(identity.guid())
                {
                    state.position = transform.position();
                    state.surface = terrain
                        .unit_submerged_liquid(state.position, liquids)?
                        .map(|liquid| liquid.surface_height);
                }
                state.model = ResourceLease::downgrade(model);
                refresh(state, model, identity, world, terrain, metadata, now)?;
            }
        }
        Ok(())
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
