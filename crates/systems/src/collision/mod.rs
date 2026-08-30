//! World and object collision queries evidenced by `AaBsp.cpp` and `Collide.cpp`.

mod aa_bsp;
mod collide;
mod liquid;
mod terrain;
mod world_model;
mod world_model_liquid;

pub use liquid::{TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample};
pub use terrain::{TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh};
pub use world_model::{
    PlacedWorldModelCollision, WorldModelCollisionError, WorldModelCollisionScene,
};
pub use world_model_liquid::{
    PlacedWorldModelLiquid, WorldModelLiquidError, WorldModelLiquidSample, WorldModelLiquidScene,
};
