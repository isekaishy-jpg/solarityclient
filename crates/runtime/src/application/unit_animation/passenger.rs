//! Vehicle passenger model state survives replacement of the unit's model.

use glam::{Mat4, Vec3};
use solarity_asset::{DecodedM2Model, VehicleCatalog, VehicleSeatDefinition};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldObjectIdentity, WorldTransform};
use solarity_rendering::M2AnimationClock;

use super::UnitAnimationBehavior;

#[derive(Clone, Copy)]
pub(in crate::application) struct UnitPassengerModelInput {
    pub parent: WorldObjectIdentity,
    pub parent_live: bool,
    pub seat: Option<VehicleSeatDefinition>,
    pub parent_pose: WorldTransform,
}

#[derive(Default)]
pub(super) struct UnitPassengerModel {
    requested: Option<(u64, i8)>,
    input: Option<UnitPassengerModelInput>,
    last_transform: Option<Mat4>,
    anchor: Option<Option<Vec3>>,
}

impl UnitAnimationBehavior {
    pub fn synchronize_passenger(
        &self,
        world: &ActiveWorld,
        vehicles: &VehicleCatalog,
        admitted_parent: Option<WorldObjectIdentity>,
    ) {
        let requested = world
            .movement_state(self.identity.guid())
            .and_then(|movement| movement.context().transport)
            .filter(|transport| transport.guid != 0)
            .map(|transport| (transport.guid, transport.seat));
        let mut state = self.passenger.borrow_mut();
        if state.requested != requested {
            state.requested = requested;
            state.input = None;
            state.anchor = None;
        }
        let Some((guid, slot)) = requested else {
            state.last_transform = None;
            return;
        };
        // An admitted generation remains bound until an explicit link change.
        // A newly arriving parent can satisfy a previously unresolved request.
        let parent = admitted_parent
            .filter(|parent| parent.guid() == guid)
            .or_else(|| state.input.map(|input| input.parent))
            .or_else(|| world.object_identity(guid));
        let Some(parent) = parent else {
            return;
        };
        let live = world.object_identity(guid) == Some(parent);
        if !live {
            if let Some(input) = state.input.as_mut() {
                input.parent_live = false;
            }
            return;
        }
        if !matches!(
            world.object_kind(guid),
            Some(ObjectKind::Unit | ObjectKind::Player)
        ) {
            state.input = None;
            return;
        }
        let seat = world
            .unit_vehicle(guid)
            .and_then(|vehicle| vehicles.passenger_seat(vehicle.definition_id(), slot));
        if !state
            .input
            .is_some_and(|input| input.seat == seat && input.parent == parent)
        {
            state.anchor = None;
        }
        state.input = Some(UnitPassengerModelInput {
            parent,
            parent_live: true,
            seat,
            parent_pose: world
                .object_transform(guid)
                .unwrap_or(WorldTransform::new(Vec3::ZERO, 0.)),
        });
    }

    pub fn passenger_input(&self) -> Option<UnitPassengerModelInput> {
        self.passenger.borrow().input
    }

    pub fn passenger_last_transform(&self) -> Option<Mat4> {
        self.passenger.borrow().last_transform
    }

    /// 748400 captures the static anchor from GetModel, including a mount.
    pub fn passenger_anchor(&self, model: &DecodedM2Model) -> Option<Vec3> {
        let mut state = self.passenger.borrow_mut();
        if let Some(anchor) = state.anchor {
            return anchor;
        }
        let anchor = state
            .input
            .and_then(|input| input.seat)
            .and_then(|seat| model.attachment(seat.passenger_attachment_id() as u32))
            .map(|attachment| attachment.position());
        state.anchor = Some(anchor);
        anchor
    }

    pub fn publish_passenger_transform(&self, transform: Mat4) {
        self.passenger.borrow_mut().last_transform = Some(transform);
    }

    /// Read the already advanced clock without taking its completion/event window.
    pub fn scene_clock(&self) -> Option<M2AnimationClock> {
        self.scene_sample
            .borrow()
            .as_ref()
            .map(|sample| sample.advance.clock)
    }
}
