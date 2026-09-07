//! Native type-15 route ownership, admission clocks, and live pose publication.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{AnimationDataCatalog, TransportCatalog};
use solarity_ecs::{ActiveWorld, GameObjectPresentation, WorldObjectIdentity};
use solarity_network::GameObjectTemplate;
use solarity_systems::{
    GameObjectPlacement, TransportRoute, TransportRouteClock, TransportRouteMotion,
    TransportRouteNode, TransportRoutePhysics, TransportRouteSample, game_object_transport_pose,
};

use super::RuntimeGameObjectError;
use super::transport_model::TransportMapModel;
use crate::application::game_object_behavior::GameObjectNotification;

/// The native MO behavior survives model replacement and keeps its own stop clock.
pub(super) struct GameObjectTransportBehavior {
    identity: WorldObjectIdentity,
    cached_state: Cell<u8>,
    route: RefCell<Option<TransportState>>,
    needs_admission: Cell<bool>,
    map_placement: Cell<Option<GameObjectPlacement>>,
    map_revision: Cell<u64>,
    pub(super) model: TransportMapModel,
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
    last_client_time_ms: u32,
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
            route: RefCell::new(None),
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
        fields: GameObjectPresentation,
        notification: GameObjectNotification,
    ) {
        if notification != GameObjectNotification::State {
            return;
        }
        let previous = self.cached_state.replace(fields.state());
        let mut state = self.route.borrow_mut();
        let Some(state) = state.as_mut().filter(|state| state.allow_stopping) else {
            return;
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
    }

    /// 710190 reapplies admission progress when a new map-model handle arrives.
    pub(super) fn detach_model(&self) {
        self.needs_admission.set(true);
        self.model.detach();
        self.map_placement.set(None);
        if let Some(state) = self.route.borrow_mut().as_mut() {
            state.sample = None;
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
        self.route
            .borrow()
            .as_ref()
            .map_or(0, |state| state.published_passenger_time_ms)
    }

    pub(super) fn animation_phase(&self) -> Option<u32> {
        self.route
            .borrow()
            .as_ref()?
            .sample
            .map(|sample| sample.animation_id)
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
        let mut state = self.route.borrow_mut();
        if state.is_none() {
            *state = Some(TransportState::new(
                catalog,
                template,
                fields,
                client_time_ms,
            )?);
        }
        let Some(state) = state.as_mut() else {
            return Ok(false);
        };
        let elapsed_ms = if self.needs_admission.replace(false) {
            state.admit(fields, raw_time_ms);
            0
        } else {
            client_time_ms.wrapping_sub(state.last_client_time_ms)
        };
        state.last_client_time_ms = client_time_ms;
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
        client_time_ms: u32,
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
            last_client_time_ms: client_time_ms,
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
