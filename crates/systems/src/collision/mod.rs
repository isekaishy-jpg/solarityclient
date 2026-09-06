//! World and object collision queries evidenced by `AaBsp.cpp` and `Collide.cpp`.

mod aa_bsp;
mod collide;
mod liquid;
mod m2_model;
mod movement_collection;
mod terrain;
mod world_model;
mod world_model_floor;
mod world_model_liquid;
mod world_model_portal;
mod world_model_registration;

pub use collide::{
    MovementCollisionPlane, MovementCollisionTriangle, MovementCollisionVolume,
    MovementFallContactKind, MovementSupportProfile, MovementSweep, MovementSweepError,
};
pub use liquid::{TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample};
pub use m2_model::{M2CollisionError, M2CollisionScene, PlacedM2Collision};
pub use movement_collection::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds, MovementTerrainChunks,
};
pub use terrain::{TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh};
pub use world_model::{
    PlacedWorldModelCollision, WorldModelCollisionError, WorldModelCollisionScene,
};
pub use world_model_floor::{WorldModelFloorHit, WorldModelFloorHits};
pub use world_model_liquid::{
    PlacedWorldModelLiquid, WorldModelLiquidError, WorldModelLiquidSample, WorldModelLiquidScene,
};
pub use world_model_portal::{WorldModelPortalHit, probe_world_model_portals};
pub use world_model_registration::{
    WorldModelRegistrationHit, WorldModelRegistrationHits, WorldModelRegistrationKind,
};
