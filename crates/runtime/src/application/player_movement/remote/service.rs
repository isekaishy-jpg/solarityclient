//! Remote unit lifetimes, packet timelines, and native collision continuation.

use std::collections::{HashMap, VecDeque};

use shipyard::Remove;
use solarity_ecs::{
    ActiveWorld, ObjectFields, ObjectKind, WorldMovementState, WorldObjectIdentity, WorldTransform,
};
use solarity_systems::{MovementBspCacheMode, MovementGroundProfile, MovementSpline};

use super::super::{
    RuntimeGameObjectPresentation, RuntimeGameplayCoordinator, RuntimeMovementGeometry,
    RuntimeMovementQuery, RuntimePlayerMovementError, RuntimeTerrainCoordinator,
};
use super::inbox::RemoteMovementInbox;
use super::state::RemoteUnit;
use crate::application::unit_water::{UnitWaterSample, UnitWaterSplash};
use crate::application::{RuntimePlayerPresentation, unit_animation::UnitMovementAnimationEvent};

/// Frame owner for remote timelines and their shared resident geometry queries.
#[derive(Default)]
pub(in crate::application) struct RuntimeRemoteMovement {
    water_samples: Vec<UnitWaterSample>,
    owners: HashMap<WorldObjectIdentity, RemoteUnit>,
    geometry: RuntimeMovementQuery,
    animation_events: VecDeque<UnitMovementAnimationEvent>,
    water_splashes: VecDeque<UnitWaterSplash>,
}

impl RuntimeRemoteMovement {
    /// Transfers completed unit registrations in the movement service's order.
    pub(in crate::application) fn take_water_samples(
        &mut self,
    ) -> impl Iterator<Item = UnitWaterSample> + '_ {
        self.water_samples.drain(..)
    }

    /// Drains the same unit splash notifications used by the local owner.
    pub(in crate::application) fn take_water_splash(&mut self) -> Option<UnitWaterSplash> {
        self.water_splashes.pop_front()
    }

    /// Collect linked simulated or spline parents, excluding unresolved wire GUIDs
    /// and owners retired by a world replacement before this service pass.
    pub(in crate::application) fn passenger_transports<'a>(
        &'a self,
        world: Option<&'a ActiveWorld>,
    ) -> impl Iterator<Item = WorldObjectIdentity> + 'a {
        self.owners.values().filter_map(move |owner| {
            let world = world?;
            let parent = owner
                .motion
                .as_ref()
                .and_then(|motion| motion.passenger)
                .or(owner.path_parent)?;
            (world.object_identity(owner.identity.guid()) == Some(owner.identity)
                && world.object_identity(parent.identity.guid()) == Some(parent.identity))
            .then_some(parent.identity)
        })
    }

    pub(in crate::application) fn take_animation_event(
        &mut self,
    ) -> Option<UnitMovementAnimationEvent> {
        self.animation_events.pop_front()
    }

    /// Consumes receipt order and publishes predictions before animation/audio.
    pub(in crate::application) fn service(
        &mut self,
        gameplay: &RuntimeGameplayCoordinator,
        terrain: &mut RuntimeTerrainCoordinator,
        objects: &RuntimeGameObjectPresentation,
        presentation: &RuntimePlayerPresentation,
        liquids: &solarity_asset::LiquidTypeCatalog,
        now_ms: u32,
    ) -> Result<(), RuntimePlayerMovementError> {
        self.water_samples.clear();
        let Some(world) = gameplay.world() else {
            self.owners.clear();
            self.animation_events.clear();
            self.water_splashes.clear();
            return Ok(());
        };
        self.owners
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
        let active_mover = gameplay.active_mover_guid();
        for guid in world.visible_unit_guids() {
            let Some(identity) = world.object_identity(guid) else {
                continue;
            };
            let Some(entity) = world.entity_by_guid(guid) else {
                continue;
            };
            let events = world
                .storage()
                .get::<&mut RemoteMovementInbox>(entity)
                .ok()
                .map(|mut inbox| std::mem::take(&mut inbox.events))
                .unwrap_or_default();
            if active_mover == Some(guid) || entity == world.local_player() {
                self.owners.remove(&identity);
                continue;
            }
            let (Some(transform), Some(movement)) =
                (world.object_transform(guid), world.movement_state(guid))
            else {
                continue;
            };
            let owner = self.owners.entry(identity).or_insert_with(|| {
                let mut owner = RemoteUnit::new(identity, transform, movement, now_ms);
                owner.path = world
                    .storage()
                    .get::<&MovementSpline>(entity)
                    .ok()
                    .map(|path| path.clone());
                owner
            });
            if owner.published != (transform, movement) {
                // Path advancement and explicit object baselines are external
                // publications. Keep clock history within this object lifetime.
                owner.published = (transform, movement);
                owner.motion = None;
                owner.path_parent = None;
                owner.path = world
                    .storage()
                    .get::<&MovementSpline>(entity)
                    .ok()
                    .map(|path| path.clone());
            }
            let target_position =
                |guid| world.object_transform(guid).map(|target| target.position());
            let dimensions = presentation
                .remote_movement_dimensions(world, guid)?
                // Original 006EBD30 dimensions before a model is available.
                .unwrap_or([
                    f32::from_bits(0x3eaa_aaab),
                    f32::from_bits(0x4001_c71c),
                    1.0,
                ]);
            let controlled = player_controlled(world, guid);
            let profile = if controlled {
                MovementGroundProfile::PlayerControlled {
                    step_height: dimensions[2],
                }
            } else {
                MovementGroundProfile::Other
            };
            // 0075E3D0 selects distinct candidate masks for ordinary units.
            let mut query_flags = if controlled { 0x0010_0111 } else { 0x0010_2111 };
            if world.object_kind(guid) == Some(ObjectKind::Player) {
                query_flags |= 0x8000;
            }
            let mut geometry = RuntimeMovementGeometry::new(
                terrain,
                world,
                objects,
                query_flags,
                MovementBspCacheMode::Enabled,
                &mut self.geometry,
            );
            owner.process(
                events,
                now_ms,
                dimensions,
                profile,
                &mut geometry,
                target_position,
            )?;
            owner.advance(now_ms, dimensions, profile, &mut geometry, target_position)?;
            self.animation_events.append(&mut owner.animation_events);
            if let Some(motion) = &mut owner.motion {
                self.animation_events.append(&mut motion.animation_events);
            }
            let (transform, movement) = owner.snapshot();
            let liquid = terrain.unit_submerged_liquid(transform.position(), liquids)?;
            let context = movement.context();
            let mut splash = false;
            if let Some(update) = (solarity_systems::MovementSwimImmersion {
                flags: movement.flags() as u32,
                secondary: (movement.flags() >> 32) as u16,
                unit_flags: world.unit_flags(guid).unwrap_or_default().primary(),
                parent_guid: movement.transport_guid().unwrap_or(0),
                locally_controlled: false,
                height: dimensions[1],
                liquid_depth: liquid.map(|sample| sample.depth),
                previous_depth: owner.previous_water_depth,
                fall_time_ms: context.fall_time_ms,
                initial_downward_speed: context.falling.map_or(0.0, |fall| fall.vertical_speed),
            })
            .evaluate()?
            {
                // The remote timeline receives swim transitions from the server;
                // the presentation-owned splash lane still advances every frame.
                owner.previous_water_depth = update.previous_depth;
                splash = update.splash;
                if update.splash {
                    self.water_splashes.push_back(UnitWaterSplash {
                        identity,
                        position: transform.position(),
                    });
                }
            }
            self.water_samples.push(UnitWaterSample {
                identity,
                transform,
                movement,
                liquid,
                height: dimensions[1],
                splash,
            });
            owner.published = (transform, movement);
            // The runtime timeline now owns path geometry. ECS keeps its compact
            // motion projection for animation and audio consumers.
            {
                world
                    .storage()
                    .run(|mut paths: shipyard::ViewMut<MovementSpline>| {
                        paths.remove(entity);
                    });
            }
            if let Ok(mut stored) = world.storage().get::<&mut WorldTransform>(entity) {
                **stored = transform;
            }
            if let Ok(mut stored) = world.storage().get::<&mut WorldMovementState>(entity) {
                **stored = movement;
            }
        }
        Ok(())
    }
}

/// Native 00716710 includes player-owned subjects and excludes charmed players.
fn player_controlled(world: &ActiveWorld, guid: u64) -> bool {
    let Some(entity) = world.entity_by_guid(guid) else {
        return false;
    };
    let Ok(fields) = world.storage().get::<&ObjectFields>(entity) else {
        return false;
    };
    let flags = fields.get(0x35);
    if flags & 2 == 0 && flags & 0xc00004 != 0 {
        return false;
    }
    let charm = u64::from(fields.get(6)) | (u64::from(fields.get(7)) << 32);
    if flags & 0x0100_0000 == 0 {
        return world.object_kind(guid) == Some(ObjectKind::Player) && charm == 0 && flags & 1 == 0;
    }
    let owner = if charm != 0 {
        charm
    } else {
        u64::from(fields.get(10)) | (u64::from(fields.get(11)) << 32)
    };
    world.object_kind(owner) == Some(ObjectKind::Player)
        && world
            .unit_flags(owner)
            .is_some_and(|flags| flags.primary() & 1 == 0)
}
