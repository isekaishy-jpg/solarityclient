//! World and object collision queries evidenced by `AaBsp.cpp` and `Collide.cpp`.

mod aa_bsp;
mod collide;
mod liquid;
mod m2_model;
mod terrain;
mod world_model;
mod world_model_liquid;

pub use collide::{
    MovementCollisionPlane, MovementCollisionTriangle, MovementCollisionVolume,
    MovementSupportProfile, MovementSweep, MovementSweepError,
};
pub use liquid::{TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample};
pub use m2_model::{M2CollisionError, M2CollisionScene, PlacedM2Collision};
pub use terrain::{TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh};
pub use world_model::{
    PlacedWorldModelCollision, WorldModelCollisionError, WorldModelCollisionScene,
};
pub use world_model_liquid::{
    PlacedWorldModelLiquid, WorldModelLiquidError, WorldModelLiquidSample, WorldModelLiquidScene,
};
