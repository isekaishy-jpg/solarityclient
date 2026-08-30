//! World and object collision queries evidenced by `AaBsp.cpp` and `Collide.cpp`.

mod aa_bsp;
mod collide;
mod liquid;
mod terrain;

pub use liquid::{TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample};
pub use terrain::{TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh};
