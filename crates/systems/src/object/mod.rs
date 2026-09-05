//! Coordinates common world-object creation, update, and removal behavior.
//!
//! `Object_C.cpp`, `ObjectAlloc.cpp`, and `ObjectMgrClient.cpp` evidence a
//! shared lifecycle responsibility. Storage details remain owned by ECS.

mod lifecycle;
mod placement;
mod types;
mod update;

pub use placement::{
    GameObjectPlacement, GameObjectPlacementError, GameObjectPlacementResolver,
    unpack_game_object_rotation,
};
pub use update::{ObjectProjectionError, project_object_fields};
