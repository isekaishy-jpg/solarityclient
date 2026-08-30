//! Position, orientation, movement flags, and trajectory component state.
//!
//! Stock separates state and behavior across `Movement_C.cpp`, `Movement.cpp`,
//! `MovementShared.cpp`, and `UnitMissileTrajectory_C.cpp`; this module owns
//! only the state vocabulary shared by movement systems.

mod movement_c;

pub use movement_c::WorldTransform;
