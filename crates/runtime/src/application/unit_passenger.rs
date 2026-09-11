//! Unit_C passenger frames, Vehicle_C matrix retention, and ancestor lifetimes.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use solarity_asset::VehicleCatalog;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldObjectIdentity, WorldTransform};
use solarity_systems::MovementTransportFrame;

use super::{RuntimeGameObjectPresentation, RuntimePlayerMovementError};

/// Shared by local movement, remote timelines, and the final world projection.
/// Model scale, pitch, and animated seat attachments do not enter Unit_C +C4.
#[derive(Default)]
pub(super) struct UnitPassengerFrames {
    vehicles: Arc<VehicleCatalog>,
    units: RefCell<HashMap<WorldObjectIdentity, UnitFrame>>,
    chain: RefCell<Vec<(WorldObjectIdentity, WorldTransform, bool)>>,
}

#[derive(Clone, Copy, Default)]
struct UnitFrame {
    parent_guid: u64,
    parent: Option<WorldObjectIdentity>,
    admitted_parent: Option<WorldObjectIdentity>,
    vehicle: Option<MovementTransportFrame>,
    computed: Option<(FrameInput, MovementTransportFrame)>,
    velocity: glam::Vec3,
}

#[derive(Clone, Copy, PartialEq)]
struct FrameInput {
    pose: [u32; 4],
    parent: Option<([u32; 16], u32)>,
}

impl UnitPassengerFrames {
    pub fn vehicles(&self) -> &VehicleCatalog {
        &self.vehicles
    }

    /// 74B900 permits yaw for a missing seat or an authored CAN_TURN seat.
    pub fn passenger_turning(
        &self,
        world: &ActiveWorld,
        parent: WorldObjectIdentity,
        seat: i8,
    ) -> bool {
        world
            .unit_vehicle(parent.guid())
            .and_then(|vehicle| self.vehicles.passenger_seat(vehicle.definition_id(), seat))
            .is_none_or(|seat| seat.flags() & 0x400 != 0)
    }
    pub fn new(vehicles: Arc<VehicleCatalog>) -> Self {
        Self {
            vehicles,
            units: RefCell::new(HashMap::new()),
            chain: RefCell::new(Vec::new()),
        }
    }

    /// A movement owner has explicitly admitted this link. This also records a
    /// legitimate reattachment after removal and GUID reuse within one frame.
    pub fn admit_parent(&self, child: WorldObjectIdentity, parent: WorldObjectIdentity) {
        let mut units = self.units.borrow_mut();
        let state = units.entry(child).or_default();
        state.parent_guid = parent.guid();
        state.parent = Some(parent);
        state.admitted_parent = Some(parent);
    }

    pub fn admitted_parent(&self, child: WorldObjectIdentity) -> Option<WorldObjectIdentity> {
        self.units
            .borrow()
            .get(&child)
            .and_then(|state| state.admitted_parent)
    }

    pub fn set_velocity(&self, identity: WorldObjectIdentity, velocity: glam::Vec3) {
        self.units
            .borrow_mut()
            .entry(identity)
            .or_default()
            .velocity = velocity;
    }

    pub fn velocity(&self, identity: WorldObjectIdentity) -> glam::Vec3 {
        self.units
            .borrow()
            .get(&identity)
            .map_or(glam::Vec3::ZERO, |state| state.velocity)
    }

    pub fn current_frame(&self, identity: WorldObjectIdentity) -> Option<MovementTransportFrame> {
        self.units
            .borrow()
            .get(&identity)
            .and_then(|state| state.computed.map(|(_, frame)| frame).or(state.vehicle))
    }

    /// Registers creation matrices before movement can replace the wire pose.
    /// Existing vehicle owners retain their matrix across definition changes.
    pub fn synchronize(
        &self,
        world: Option<&ActiveWorld>,
        objects: &RuntimeGameObjectPresentation,
    ) -> Result<(), RuntimePlayerMovementError> {
        let Some(world) = world else {
            self.units.borrow_mut().clear();
            return Ok(());
        };
        let guids = world.visible_unit_guids();
        {
            let mut units = self.units.borrow_mut();
            units.retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
            for &guid in &guids {
                if let Some(identity) = world.object_identity(guid) {
                    register(&mut units, world, identity)?;
                }
            }
        }
        // 73AB20/758130 updates vehicle matrices even without a visible rider.
        for guid in guids {
            if world.unit_vehicle(guid).is_some()
                && let Some(identity) = world.object_identity(guid)
            {
                self.resolve(world, objects, identity)?;
            }
        }
        Ok(())
    }

    /// Walks the live ancestry iteratively; a retired admitted generation cannot
    /// be replaced merely because the same GUID appears again.
    pub fn resolve(
        &self,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
        identity: WorldObjectIdentity,
    ) -> Result<Option<MovementTransportFrame>, RuntimePlayerMovementError> {
        let mut units = self.units.borrow_mut();
        let mut chain = self.chain.borrow_mut();
        chain.clear();
        let mut current = identity;
        let mut parent_frame = loop {
            if world.object_identity(current.guid()) != Some(current) {
                break None;
            }
            if !is_unit(world, current) {
                break objects.object_movement_frame(current)?;
            }
            if chain.iter().any(|(visited, _, _)| *visited == current) {
                return Err(RuntimePlayerMovementError::PassengerCycle);
            }
            let Some(transform) = world.object_transform(current.guid()) else {
                break None;
            };
            register(&mut units, world, current)?;
            let Some(state) = units.get(&current) else {
                return Ok(None);
            };
            let transport = world
                .movement_state(current.guid())
                .and_then(|movement| movement.context().transport)
                .filter(|transport| transport.guid != 0);
            let local = transport.map_or(transform, |transport| {
                WorldTransform::new(transport.position, transport.orientation)
            });
            let vehicle = world
                .unit_vehicle(current.guid())
                .is_some_and(|vehicle| self.vehicles.vehicle(vehicle.definition_id()).is_some());
            chain.push((current, local, vehicle));
            if state.parent_guid == 0 {
                break None;
            }
            let Some(parent) = state.parent else {
                break None;
            };
            current = parent;
        };
        for (identity, local, valid_vehicle) in chain.drain(..).rev() {
            let Some(state) = units.get_mut(&identity) else {
                return Ok(None);
            };
            if state.parent_guid != 0 && parent_frame.is_none() {
                // 758130 leaves a valid vehicle's cached matrix untouched when
                // its parent lookup fails. Generic units have no such cache.
                parent_frame =
                    if let Some(cached) = valid_vehicle.then_some(state.vehicle).flatten() {
                        // Facing is a separate virtual getter: 74B590 contributes
                        // zero on failed lookup, then 4F42A0 still wraps local yaw.
                        let zero = MovementTransportFrame::new(glam::Mat4::IDENTITY, 0.)?;
                        Some(MovementTransportFrame::new(
                            cached.world_matrix(),
                            zero.world_orientation(local.orientation()),
                        )?)
                    } else {
                        None
                    };
                continue;
            }
            let input = FrameInput {
                pose: [
                    local.position().x.to_bits(),
                    local.position().y.to_bits(),
                    local.position().z.to_bits(),
                    local.orientation().to_bits(),
                ],
                parent: parent_frame.map(|parent| {
                    (
                        parent.world_matrix().to_cols_array().map(f32::to_bits),
                        parent.facing().to_bits(),
                    )
                }),
            };
            let frame = if let Some((_, frame)) = state.computed.filter(|(key, _)| *key == input) {
                frame
            } else {
                MovementTransportFrame::unit(local.position(), local.orientation(), parent_frame)?
            };
            state.computed = Some((input, frame));
            if valid_vehicle {
                state.vehicle = Some(frame);
            }
            parent_frame = Some(frame);
        }
        Ok(parent_frame)
    }
}

pub(super) fn is_unit(world: &ActiveWorld, identity: WorldObjectIdentity) -> bool {
    world.object_identity(identity.guid()) == Some(identity)
        && matches!(
            world.object_kind(identity.guid()),
            Some(ObjectKind::Unit | ObjectKind::Player)
        )
}

fn register(
    units: &mut HashMap<WorldObjectIdentity, UnitFrame>,
    world: &ActiveWorld,
    identity: WorldObjectIdentity,
) -> Result<(), RuntimePlayerMovementError> {
    let Some(transform) = world.object_transform(identity.guid()) else {
        return Ok(());
    };
    let guid = world
        .movement_state(identity.guid())
        .and_then(|movement| movement.transport_guid())
        .filter(|guid| *guid != 0)
        .unwrap_or(0);
    let state = units.entry(identity).or_insert(UnitFrame {
        parent_guid: 0,
        parent: None,
        admitted_parent: None,
        vehicle: None,
        computed: None,
        velocity: glam::Vec3::ZERO,
    });
    if state.vehicle.is_none()
        && let Some(vehicle) = world.unit_vehicle(identity.guid())
    {
        // 757FA0 uses the creation WORLD pose; +50 initial facing is a separate lane.
        let transform = vehicle.initial_transform().unwrap_or(transform);
        state.vehicle = Some(MovementTransportFrame::unit(
            transform.position(),
            transform.orientation(),
            None,
        )?);
    }
    if state.parent_guid != guid {
        state.parent_guid = guid;
        state.parent = None;
        state.admitted_parent = None;
    }
    if state.parent.is_none() && guid != 0 {
        state.parent = world.object_identity(guid);
    }
    Ok(())
}
