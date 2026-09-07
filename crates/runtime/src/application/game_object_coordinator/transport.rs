//! Native transport behavior ownership, admission clocks, and pose publication.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{AnimationDataCatalog, DecodedM2Model, TransportCatalog};
use solarity_ecs::{ActiveWorld, GameObjectPresentation, WorldObjectIdentity};
use solarity_network::GameObjectTemplate;
use solarity_systems::{
    GameObjectPlacement, GameObjectPlacementResolver, TransportRoute, TransportRouteClock,
    TransportRouteMotion, TransportRouteNode, TransportRoutePhysics, TransportRouteSample,
    game_object_transport_pose,
};

use super::RuntimeGameObjectError;
use super::transport_animation::{AnimationFrame, TransportAnimationState};
use super::transport_model::TransportMapModel;
use crate::application::game_object_behavior::GameObjectNotification;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

/// The native MO behavior survives model replacement and keeps its own stop clock.
pub(super) struct GameObjectTransportBehavior {
    identity: WorldObjectIdentity,
    cached_state: Cell<u8>,
    motion: TransportMotion,
    needs_admission: Cell<bool>,
    map_placement: Cell<Option<GameObjectPlacement>>,
    map_revision: Cell<u64>,
    pub(super) model: TransportMapModel,
}

/// Native type 11 owns a creation-time animation; type 15 awaits its route template.
enum TransportMotion {
    Route(RefCell<Option<TransportState>>),
    Animation(RefCell<Option<TransportAnimationState>>),
}

/// Template-owned route geometry and the mutable clock at behavior+0x40.
struct TransportState {
    route: TransportRoute,
    clock: TransportRouteClock,
    allow_stopping: bool,
    /// Behavior+0x30 is written only after a successful current-map sample.
    published_time_ms: u32,
    /// Behavior+0x3C is the converted passenger clock returned by virtual A8.
    published_passenger_time_ms: u32,
    sample: Option<TransportRouteSample>,
}

impl GameObjectTransportBehavior {
    /// Capture the creation state before later packet notifications can replace it.
    pub(super) fn new(
        identity: WorldObjectIdentity,
        fields: GameObjectPresentation,
        animations: Arc<AnimationDataCatalog>,
    ) -> Self {
        Self {
            identity,
            cached_state: Cell::new(fields.state()),
            motion: if fields.object_type() == 11 {
                TransportMotion::Animation(RefCell::new(None))
            } else {
                TransportMotion::Route(RefCell::new(None))
            },
            needs_admission: Cell::new(true),
            map_placement: Cell::new(None),
            map_revision: Cell::new(0),
            model: TransportMapModel::new(animations),
        }
    }

    /// 7101C0 toggles repeated states using the last published raw route time.
    /// Flags and progress callbacks for this behavior are native no-ops.
    pub(super) fn notify(
        &self,
        world: &mut ActiveWorld,
        fields: GameObjectPresentation,
        notification: GameObjectNotification,
        catalog: Option<&TransportCatalog>,
        client_time_ms: u32,
        resolver: &mut GameObjectPlacementResolver,
    ) -> Result<(), RuntimeGameObjectError> {
        if let TransportMotion::Animation(state) = &self.motion {
            let guid = self.identity.guid();
            let movement = world
                .game_object_movement(guid)
                .ok_or(RuntimeGameObjectError::MissingTransportCreation { guid })?;
            let raw_time_ms = movement.transport_clock_ms(client_time_ms);
            let mut state = state.borrow_mut();
            if notification == GameObjectNotification::Initialize && state.is_none() {
                *state = Some(TransportAnimationState::new(
                    world,
                    self.identity,
                    catalog.ok_or(RuntimeGameObjectError::MissingTransportCatalog)?,
                    fields,
                    raw_time_ms,
                    resolver,
                )?);
            } else if notification == GameObjectNotification::State {
                let previous = self.cached_state.replace(fields.state());
                // 711050 admits repeated callbacks only for type 15. This cache
                // is GO+204, distinct from the animation clock's retained state.
                if previous != fields.state() {
                    state
                        .as_mut()
                        .ok_or(RuntimeGameObjectError::MissingTransportCreation { guid })?
                        .notify_state(fields, previous, raw_time_ms);
                }
            }
            return Ok(());
        }
        if notification != GameObjectNotification::State {
            return Ok(());
        }
        let previous = self.cached_state.replace(fields.state());
        let TransportMotion::Route(route) = &self.motion else {
            return Ok(());
        };
        let mut state = route.borrow_mut();
        let Some(state) = state.as_mut().filter(|state| state.allow_stopping) else {
            return Ok(());
        };
        if previous == fields.state() {
            state.clock.set_motion(
                &state.route,
                state.published_time_ms,
                motion(fields.state() != 1),
            );
        }
        state.clock.set_motion(
            &state.route,
            state.published_time_ms,
            motion(fields.state() == 1),
        );
        Ok(())
    }

    /// 710190 reapplies admission progress when a new map-model handle arrives.
    pub(super) fn detach_model(&self) {
        self.needs_admission.set(true);
        self.model.detach();
        self.map_placement.set(None);
        match &self.motion {
            TransportMotion::Route(route) => {
                if let Some(state) = route.borrow_mut().as_mut() {
                    state.sample = None;
                }
            }
            TransportMotion::Animation(animation) => {
                if let Some(state) = animation.borrow_mut().as_mut() {
                    state.detach_model();
                }
            }
        }
    }

    pub(super) fn needs_map_placement(&self) -> bool {
        self.needs_admission.get()
    }

    pub(super) fn set_map_placement(&self, placement: Option<GameObjectPlacement>) {
        self.map_placement.set(placement);
        self.map_revision
            .set(self.map_revision.get().wrapping_add(1));
    }

    pub(super) fn map_placement(&self) -> Option<GameObjectPlacement> {
        self.map_placement.get()
    }

    pub(super) fn map_revision(&self) -> u64 {
        self.map_revision.get()
    }

    /// 959D00 returns the last successful 7134A0/714240 phase, initially zero.
    pub(super) fn passenger_time_ms(&self) -> u32 {
        match &self.motion {
            TransportMotion::Route(route) => route
                .borrow()
                .as_ref()
                .map_or(0, |state| state.published_passenger_time_ms),
            TransportMotion::Animation(animation) => animation
                .borrow()
                .as_ref()
                .map_or(0, |state| state.passenger_time_ms),
        }
    }

    pub(super) fn animation_phase(&self) -> Option<u32> {
        match &self.motion {
            TransportMotion::Route(route) => route
                .borrow()
                .as_ref()?
                .sample
                .map(|sample| sample.animation_id),
            TransportMotion::Animation(animation) => animation.borrow().as_ref()?.sequence_id(),
        }
    }

    /// Dispatch every key transition in native sampling order. A stalled type-11
    /// frame can select two different sequences before the scene is presented.
    pub(super) fn attach_model(
        &self,
        display_id: u32,
        model: &Arc<DecodedM2Model>,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let TransportMotion::Animation(animation) = &self.motion else {
            return self.model.attach(
                display_id,
                model,
                self.animation_phase(),
                scene_time_ms,
                random,
            );
        };
        self.model
            .attach(display_id, model, None, scene_time_ms, random)?;
        if let Some(state) = animation.borrow_mut().as_mut() {
            while let Some(sequence) = state.take_sequence() {
                self.model
                    .attach(display_id, model, Some(sequence), scene_time_ms, random)?;
            }
        }
        Ok(())
    }

    /// Dedicated WMO handles consume key changes without an M2 playback owner.
    pub(super) fn discard_model_sequences(&self) {
        if let TransportMotion::Animation(animation) = &self.motion
            && let Some(state) = animation.borrow_mut().as_mut()
        {
            state.discard_sequences();
        }
    }

    pub(super) fn is_animation(&self) -> bool {
        matches!(&self.motion, TransportMotion::Animation(_))
    }

    /// Type-11 frames run even before asynchronous resource and template admission.
    pub(super) fn advance_animation(
        &self,
        world: &mut ActiveWorld,
        frame: AnimationFrame,
        resolver: &mut GameObjectPlacementResolver,
    ) -> Result<(), RuntimeGameObjectError> {
        let TransportMotion::Animation(animation) = &self.motion else {
            return Ok(());
        };
        let guid = self.identity.guid();
        let fields = world
            .game_object_presentation(guid)
            .ok_or(RuntimeGameObjectError::MissingTransportCreation { guid })?;
        animation
            .borrow_mut()
            .as_mut()
            .ok_or(RuntimeGameObjectError::MissingTransportCreation { guid })?
            .advance(world, guid, fields, frame, resolver)
    }

    /// 70B5C0 replaces the new type-11 handle's yaw matrix with virtual C4's pose.
    pub(super) fn admit_animation_map(&self, placement: GameObjectPlacement) {
        self.set_map_placement(Some(placement));
        self.needs_admission.set(false);
    }

    /// 711B50 builds one route from the exact template and stored DBC controls.
    /// Neither later LEVEL updates nor ordinary sequence notifications rebuild it.
    /// Returns whether 7134A0 wrote a current-map pose, including station samples.
    pub(super) fn advance(
        &self,
        world: &mut ActiveWorld,
        catalog: &TransportCatalog,
        template: &GameObjectTemplate,
        client_time_ms: u32,
        elapsed_ms: u32,
    ) -> Result<bool, RuntimeGameObjectError> {
        let guid = self.identity.guid();
        if world.object_identity(guid) != Some(self.identity) {
            return Ok(false);
        }
        let Some(fields) = world.game_object_presentation(guid) else {
            return Ok(false);
        };
        let Some(movement) = world.game_object_movement(guid) else {
            return Ok(false);
        };
        let raw_time_ms = movement.transport_clock_ms(client_time_ms);
        let TransportMotion::Route(route) = &self.motion else {
            return Ok(false);
        };
        let mut state = route.borrow_mut();
        if state.is_none() {
            *state = Some(TransportState::new(catalog, template, fields)?);
        }
        let Some(state) = state.as_mut() else {
            return Ok(false);
        };
        let elapsed_ms = if self.needs_admission.replace(false) {
            state.admit(fields, raw_time_ms);
            0
        } else {
            elapsed_ms
        };
        let clock_ms = state.clock.clock_ms(&state.route, raw_time_ms, elapsed_ms);
        let Some(sample) = state.route.sample(clock_ms) else {
            return Ok(false);
        };
        // 7134A0 returns before changing the pose, phase, or published raw time
        // when the next route section belongs to another map. The server owns
        // world transfer; sampling the route cannot synthesize that protocol.
        if sample.map_id != world.map_id().value() {
            return Ok(false);
        }
        let pose =
            game_object_transport_pose(sample.position, sample.yaw, sample.pitch, sample.roll)?;
        world.update_game_object_animated_pose(guid, pose)?;
        state.published_time_ms = raw_time_ms;
        state.published_passenger_time_ms = clock_ms;
        state.sample = Some(sample);
        Ok(true)
    }
}

impl TransportState {
    /// Preserve native signed template conversion and the unconditional period override.
    fn new(
        catalog: &TransportCatalog,
        template: &GameObjectTemplate,
        fields: GameObjectPresentation,
    ) -> Result<Self, RuntimeGameObjectError> {
        let properties = template.properties();
        let nodes: Vec<_> = catalog
            .path(properties[0])
            .iter()
            .map(|node| TransportRouteNode {
                map_id: node.map_id,
                position: Vec3::from_array(node.position),
                flags: node.flags,
                delay_seconds: node.delay_seconds,
                arrival_event: node.arrival_event,
                departure_event: node.departure_event,
            })
            .collect();
        let mut route = TransportRoute::new(
            &nodes,
            properties[1] as i32 as f32,
            properties[2] as i32 as f32,
        )?;
        route.set_period_ms(fields.transport_period_ms());
        route.set_physics(
            catalog
                .physics(properties[5])
                .map(|row| TransportRoutePhysics::new(row.parameters))
                .transpose()?,
        );
        Ok(Self {
            route,
            clock: TransportRouteClock::default(),
            allow_stopping: properties[8] != 0,
            published_time_ms: 0,
            published_passenger_time_ms: 0,
            sample: None,
        })
    }

    /// 7100D0 uses every ushort fraction, including FFFF, before applying motion.
    fn admit(&mut self, fields: GameObjectPresentation, raw_time_ms: u32) {
        if !self.allow_stopping {
            return;
        }
        self.clock.synchronize_progress(
            &self.route,
            raw_time_ms,
            (fields.dynamic_word() >> 16) as u16,
        );
        if fields.dynamic_word() & 0x10 != 0 {
            self.clock.freeze_at_station(&self.route, raw_time_ms);
        } else {
            self.clock
                .set_motion(&self.route, raw_time_ms, motion(fields.state() == 1));
        }
    }
}

/// Native boolean stop arguments expressed as a motion request at the systems boundary.
fn motion(stop: bool) -> TransportRouteMotion {
    if stop {
        TransportRouteMotion::StopAtStation
    } else {
        TransportRouteMotion::Moving
    }
}
