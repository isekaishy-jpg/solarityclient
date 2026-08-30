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

pub use movement::WorldTransform;
pub use object::{ObjectFields, ObjectGuid, ObjectKind, ObjectPresentation};
pub use player::{LocalPlayer, PlayerAppearance, PlayerIdentity};
pub use unit::{UnitFlags, UnitIdentity, UnitPresentation, UnitVitals};
pub use world::{ActiveWorld, WorldBootstrap, WorldMapId, WorldStateError};
