//! Liquid, floor lighting, area and sound consumers of native registration.

use super::super::{
    MovementRootReference, RuntimeStaticMovementResidency, RuntimeTerrainCoordinator,
    RuntimeWorldModelMovementOwner, SceneAddress,
};
use super::{
    RuntimeMovementRegistrationError, RuntimeMovementRegistrationQuery, UnitWorldModelLocation,
};
use glam::Vec3;
use solarity_asset::TerrainMap;
use solarity_systems::{MovementBspCacheMode, MovementCollisionBounds, PlacedM2Collision};

impl RuntimeTerrainCoordinator {
    /// Rebuilds an ordinary model's shared world registration for its scene
    /// callbacks. Unit and GameObject owners retain their distinct native lists.
    pub(in crate::application) fn register_model_scene(
        &mut self,
        position: Vec3,
        bounds: MovementCollisionBounds,
        game_object: Option<&PlacedM2Collision>,
        scratch: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        scratch.clear();
        let Some(active) = self.active.as_mut() else {
            return Ok(());
        };
        if let Some(model) = game_object {
            active.register_game_object_movement(model, MovementBspCacheMode::Enabled, scratch)?;
        } else {
            active.register_unit_liquid(position, bounds, scratch)?;
        }
        Ok(())
    }

    pub(in crate::application) fn model_light_revision(&self) -> u64 {
        self.active
            .as_ref()
            .map_or(0, |active| active.movement.lighting.revision())
    }

    /// MODR membership and group flags depend on source residency, not root motion.
    pub(in crate::application) fn doodad_light_revision(&self) -> u64 {
        self.active
            .as_ref()
            .map_or(0, |active| active.movement.lighting.topology_revision())
    }

    /// Retains a stationary unit's floor/liquid query across unrelated root motion.
    pub(in crate::application) fn unit_light_changed(&self, previous: u64, position: Vec3) -> bool {
        self.active
            .as_ref()
            .is_none_or(|active| active.movement.lighting.affects_unit(previous, position))
    }

    /// Uses the native Unit/MapObject registration's fallback face for floor color.
    pub(in crate::application) fn model_floor_light(
        &mut self,
        position: Vec3,
        model: Option<&PlacedM2Collision>,
        scratch: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<
        (bool, Option<solarity_systems::WorldModelFloorLight>, bool),
        RuntimeMovementRegistrationError,
    > {
        let Some(active) = self.active.as_mut() else {
            return Ok((false, None, false));
        };
        let selection = if let Some(model) = model {
            scratch.clear();
            active.register_game_object_movement(model, MovementBspCacheMode::Enabled, scratch)?;
            let Some(selection) = scratch.selection() else {
                return Ok((false, None, false));
            };
            selection
        } else {
            active.unit_registration(position)?
        };
        if !selection
            .selected()
            .is_some_and(|candidate| candidate.hit().is_interior())
        {
            // 7A1BC0 applies baked terrain shadows only outside interiors.
            // Unit registration also excludes a selected exterior WMO through
            // +C bit 0x200; MapObject registration does not set that bit.
            let shadow = (model.is_some() || selection.selected().is_none())
                && solarity_systems::WorldEntityTerrainShadowPoint::new(position)
                    .and_then(|point| {
                        let tile = active.tile_at(point.tile())?;
                        let chunk = tile
                            .decoded
                            .chunks()
                            .iter()
                            .find(|chunk| chunk.index() == point.chunk())?;
                        Some(point.is_shadowed(chunk.shadow_map()?))
                    })
                    .unwrap_or(false);
            return Ok((false, None, shadow));
        }
        let Some(candidate) = selection.fallback().into_iter().flatten().next() else {
            return Ok((false, None, false));
        };
        let reference = active
            .movement
            .roots
            .iter()
            .find(|reference| reference.owner() == candidate.owner())
            .copied()
            .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
        let placement = active.registration_root_mut(reference)?;
        let hit = candidate.hit();
        let group = placement
            .model()
            .groups()
            .get(hit.group_index())
            .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
        let interior = group.flags() & 0x48 == 0;
        // 7C2A70/7C2E70 reconstruct XY at the entity center. The dominant-axis
        // interpolation consumes this point; the native Z omits the start bias.
        let (center, start_z) = model.map_or((position, position.z + 0.1), |model| {
            let center = model.collision_center();
            (
                center,
                (f64::from(center.z) + 4.)
                    .min(f64::from(model.render_bounds().maximum().z) + f64::from(0.1_f32))
                    as f32,
            )
        });
        let point = Vec3::new(
            center.x,
            center.y,
            (f64::from(center.z)
                - (f64::from(center.z - 1000.) - f64::from(start_z)) * f64::from(hit.fraction()))
                as f32,
        );
        Ok((
            interior,
            if interior {
                placement.sample_group_floor_light(hit.group_index(), hit.face(), point)?
            } else {
                None
            },
            false,
        ))
    }

    /// MODD remains exterior if any registered group is exterior (7BF7F0).
    pub(in crate::application) fn doodad_interior_lighting(
        &mut self,
        owner: RuntimeWorldModelMovementOwner,
        doodad_index: usize,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(false);
        };
        let Some(reference) = active
            .movement
            .roots
            .iter()
            .find(|root| root.owner() == owner)
            .copied()
        else {
            return Ok(false);
        };
        let placement = active.registration_root_mut(reference)?;
        let mut interior = false;
        for group in placement.model().groups() {
            if group
                .doodad_references()
                .iter()
                .any(|&index| usize::from(index) == doodad_index)
            {
                if group.flags() & 0x48 != 0 {
                    return Ok(false);
                }
                interior = true;
            }
        }
        Ok(interior)
    }

    /// Resolves 7A1640's WMO tuple from the same floor/portal banks as Unit_C.
    pub(in crate::application) fn unit_world_model_location(
        &mut self,
        position: Vec3,
    ) -> Result<Option<UnitWorldModelLocation>, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let selection = active.unit_registration(position)?;
        for candidate in selection.primary().into_iter().flatten() {
            let reference = active
                .movement
                .roots
                .iter()
                .find(|reference| reference.owner() == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            // 77F090/7A1640 skip exterior groups on transformed roots (flag 0x400).
            let transformed = matches!(reference, MovementRootReference::GameObject(_));
            if transformed && !candidate.hit().is_interior() {
                continue;
            }
            let name_set = match reference {
                // 7BF120 initializes replicated roots' name-set word +0x104 to zero.
                MovementRootReference::GameObject(_) => 0,
                MovementRootReference::Static(reference) => {
                    let scene = match reference.scene {
                        SceneAddress::Global => {
                            &active
                                .global_world_model
                                .as_ref()
                                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                                .world_models
                        }
                        SceneAddress::Tile(index) => {
                            &active
                                .tile_at(index)
                                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                                .world_models
                        }
                    };
                    u32::from(
                        scene
                            .placements()
                            .get(reference.placement)
                            .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                            .name_set(),
                    )
                }
            };
            let placement = active.registration_root_mut(reference)?;
            let group = placement
                .model()
                .groups()
                .get(candidate.hit().group_index())
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            return Ok(Some(UnitWorldModelLocation {
                key: solarity_asset::WorldModelAreaKey {
                    root_id: placement.model().world_model_id(),
                    name_set,
                    group_id: group.area_table_id() as i32,
                },
                world_model_only: candidate.hit().is_interior(),
                area_override: !transformed,
            }));
        }
        Ok(None)
    }

    /// Resolves Unit_C's ground sound type with native floor and texture selection.
    pub(in crate::application) fn unit_ground_sound_type(
        &mut self,
        position: Vec3,
        sounds: &solarity_asset::MovementSoundCatalog,
    ) -> Result<u32, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(u32::MAX);
        };
        let selection = active.unit_registration(position)?;
        // 0x007C2A70 uses the corresponding fallback face, including its
        // absent-face sentinel after portal-only registration.
        if let Some(candidate) = selection.fallback().into_iter().flatten().next() {
            let reference = active
                .movement
                .roots
                .iter()
                .find(|reference| reference.owner() == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            let placement = active.registration_root_mut(reference)?;
            let hit = candidate.hit();
            let material = hit
                .face()
                .and_then(|face| {
                    placement
                        .model()
                        .groups()
                        .get(hit.group_index())?
                        .polygons()
                        .get(usize::from(face))
                })
                .and_then(|polygon| {
                    placement
                        .model()
                        .materials()
                        .get(usize::from(polygon.material_id()))
                });
            return Ok(material.map_or(u32::MAX, |material| material.ground_type()));
        }
        let Some((index, chunk_index, [x, y])) =
            TerrainMap::sound_cell_at_world_position(position.x, position.y)
        else {
            return Ok(u32::MAX);
        };
        let Some(tile) = active.tile_at(index) else {
            return Ok(u32::MAX);
        };
        let effect = tile
            .decoded
            .chunks()
            .iter()
            .find(|chunk| chunk.index() == chunk_index)
            .and_then(|chunk| chunk.ground_effect_at(x, y));
        Ok(effect
            .and_then(|effect| sounds.ground_effect_terrain(effect))
            .unwrap_or(u32::MAX))
    }

    /// Resolves GameObject M2 destinations against the admitted map scene.
    ///
    /// Uses native floor/portal banks, upward retry, terrain occlusion, interior
    /// group selection, and direct chunk rounding. Replicated WMO roots enter
    /// native transformed-root banks after GameObject movement synchronization.
    ///
    /// # Errors
    /// Returns [`RuntimeMovementRegistrationError`] for invalid geometry or an
    /// invalid admitted reference. All pending/error results clear the output.
    pub fn register_game_object_movement(
        &mut self,
        map_id: u32,
        model: &PlacedM2Collision,
        cache: MovementBspCacheMode,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeMovementRegistrationError> {
        output.clear();
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.map_id() == map_id)
        else {
            return Ok(RuntimeStaticMovementResidency::PendingMap);
        };
        let result = active.register_game_object_movement(model, cache, output);
        if matches!(result, Ok(RuntimeStaticMovementResidency::Ready)) {
            output.map_id = Some(map_id);
        } else {
            output.clear();
        }
        result
    }
}
