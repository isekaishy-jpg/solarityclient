//! Packet and object-update order retained until the world movement phase.

use std::collections::VecDeque;

use shipyard::Component;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldMovementState, WorldTransform};
use solarity_network::{MonsterMove, RemoteMovement};
use solarity_systems::MovementSpline;

/// Receipt-ordered movement inputs destroyed with their ECS subject.
#[derive(Default, Component)]
pub(crate) struct RemoteMovementInbox {
    pub events: VecDeque<RemoteMovementInput>,
}

/// Object baselines, ordinary commands, and path replacement share one ordering.
pub(crate) enum RemoteMovementInput {
    Baseline {
        transform: WorldTransform,
        movement: WorldMovementState,
        spline: Option<Box<MovementSpline>>,
        receipt_ms: u32,
    },
    Command {
        message: RemoteMovement,
        receipt_ms: u32,
    },
    Path {
        message: MonsterMove,
        receipt_ms: u32,
        stop_distance_tolerance: f32,
    },
}

impl RemoteMovementInput {
    pub fn receipt_ms(&self) -> u32 {
        match self {
            Self::Baseline { receipt_ms, .. }
            | Self::Command { receipt_ms, .. }
            | Self::Path { receipt_ms, .. } => *receipt_ms,
        }
    }
}

/// Retains paths in the same receipt order as ordinary movement snapshots.
pub(crate) fn receive_path(
    world: &mut ActiveWorld,
    message: MonsterMove,
    receipt_ms: u32,
    stop_distance_tolerance: f32,
) -> bool {
    if !matches!(
        world.object_kind(message.guid),
        Some(ObjectKind::Unit | ObjectKind::Player)
    ) {
        return false;
    }
    let guid = message.guid;
    push(
        world,
        guid,
        RemoteMovementInput::Path {
            message,
            receipt_ms,
            stop_distance_tolerance,
        },
    );
    true
}

/// 00741B60 consumes unknown GUIDs without manufacturing a movement subject.
pub(crate) fn receive(
    world: &mut ActiveWorld,
    message: RemoteMovement,
    receipt_ms: u32,
    active_mover: u64,
) -> bool {
    if message.guid == active_mover
        || !matches!(
            world.object_kind(message.guid),
            Some(ObjectKind::Unit | ObjectKind::Player)
        )
    {
        return false;
    }
    let guid = message.guid;
    push(
        world,
        guid,
        RemoteMovementInput::Command {
            message,
            receipt_ms,
        },
    );
    true
}

/// Freezes an object-update baseline before a later packet can replace its path.
pub(crate) fn baseline(world: &mut ActiveWorld, guid: u64, receipt_ms: u32) {
    let (Some(transform), Some(movement)) =
        (world.object_transform(guid), world.movement_state(guid))
    else {
        return;
    };
    // A later baseline can replace the ECS path before the movement phase.
    // Retain this baseline's own geometry to replay the exact packet order.
    let spline = world.entity_by_guid(guid).and_then(|entity| {
        world
            .storage()
            .get::<&MovementSpline>(entity)
            .ok()
            .map(|spline| spline.clone())
    });
    push(
        world,
        guid,
        RemoteMovementInput::Baseline {
            transform,
            movement,
            spline: spline.map(Box::new),
            receipt_ms,
        },
    );
}

/// Attaches the inbox only to an already admitted object lifetime.
fn push(world: &mut ActiveWorld, guid: u64, input: RemoteMovementInput) {
    let Some(entity) = world.entity_by_guid(guid) else {
        return;
    };
    if let Ok(mut inbox) = world.storage().get::<&mut RemoteMovementInbox>(entity) {
        inbox.events.push_back(input);
        return;
    }
    world.storage_mut().add_component(
        entity,
        (RemoteMovementInbox {
            events: VecDeque::from([input]),
        },),
    );
}
