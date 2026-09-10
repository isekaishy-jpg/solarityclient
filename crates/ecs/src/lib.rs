//! Entity, component, world-state, and scheduling primitives.

mod corpse;
mod creature;
mod dynamic_object;
mod effect;
mod game_object;
mod item;
mod minigame;
mod missile;
mod movement;
mod object;
mod player;
mod spell;
mod trade;
mod unit;
mod vehicle;
mod view;
mod world;

pub use game_object::{
    GameObjectAnimatedPose, GameObjectMovement, GameObjectPresentation, GameObjectTransport,
};
pub use movement::{
    WorldMovementContext, WorldMovementFall, WorldMovementSpeeds, WorldMovementSpline,
    WorldMovementState, WorldMovementTransport, WorldTransform,
};
pub use object::{ObjectFields, ObjectGuid, ObjectKind, ObjectPresentation};
pub use player::{
    LocalPlayer, PLAYER_EQUIPMENT_SLOT_COUNT, PlayerAppearance, PlayerEquipment,
    PlayerEquipmentSlot, PlayerIdentity, PlayerLocalStandState, PlayerMoney, PlayerProgression,
    VisibleEquipmentItem,
};
pub use unit::{
    UNIT_PRIMARY_STAT_COUNT, UnitAnimationTier, UnitAttackTarget, UnitAura, UnitAuras, UnitFlags,
    UnitHealthPrediction, UnitIdentity, UnitPresentation, UnitSheathState, UnitStats, UnitVitals,
};
pub use vehicle::UnitVehicle;
pub use view::PlayerViewState;
pub use world::WorldStateValues;
pub use world::{ActiveWorld, WorldBootstrap, WorldMapId, WorldObjectIdentity, WorldStateError};
