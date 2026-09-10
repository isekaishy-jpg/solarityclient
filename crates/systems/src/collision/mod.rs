//! World and object collision queries evidenced by `AaBsp.cpp` and `Collide.cpp`.

mod aa_bsp;
mod collide;
mod liquid;
mod m2_model;
mod movement_collection;
mod terrain;
mod world_model;
mod world_model_camera;
mod world_model_floor;
mod world_model_fog;
mod world_model_lighting;
mod world_model_liquid;
mod world_model_portal;
mod world_model_registration;
mod world_model_registration_scene;
mod world_model_visibility;
mod world_model_water_ray;

pub use collide::{
    MovementCollisionPlane, MovementCollisionTriangle, MovementCollisionVolume,
    MovementFallContactKind, MovementSupportProfile, MovementSweep, MovementSweepError,
};
pub use liquid::{
    SubmergedLiquid, SubmergedLiquidError, TerrainLiquidError, TerrainLiquidMesh,
    TerrainLiquidSample,
};
pub use m2_model::{M2CollisionError, M2CollisionScene, PlacedM2Collision};
pub use movement_collection::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds, MovementTerrainChunks,
    append_terrain_liquid_movement,
};
pub use terrain::{
    TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh, TerrainRegistrationPoint,
};
pub use world_model::{
    PlacedWorldModelCollision, WorldModelCollisionError, WorldModelCollisionScene,
};
pub use world_model_camera::{
    WorldModelCameraRegistration, WorldModelCameraRegistrationQuery,
    WorldModelCameraSceneRegistration,
};
pub use world_model_floor::{WorldModelFloorHit, WorldModelFloorHits};
pub use world_model_fog::WorldModelFogEnvironment;
pub use world_model_lighting::{WorldModelFloorLight, world_model_doodad_light_colors};
pub use world_model_liquid::{
    PlacedWorldModelLiquid, WorldModelLiquidError, WorldModelLiquidSample, WorldModelLiquidScene,
};
pub use world_model_portal::{WorldModelPortalHit, probe_world_model_portals};
pub use world_model_registration::{
    WorldModelRegistrationHit, WorldModelRegistrationHits, WorldModelRegistrationKind,
};
pub use world_model_registration_scene::{
    WorldModelRegistrationCandidate, WorldModelRegistrationQuery, WorldModelRegistrationSelection,
};
pub use world_model_visibility::{
    WorldModelBatchVisibilityQuery, WorldModelCameraSceneQuery, WorldModelExteriorPortalWindow,
    WorldModelPortalProjectionFrame, WorldModelPortalProjector, WorldModelSceneFog,
    WorldModelSceneGroupVisit, WorldModelSceneVisibilityEvent, WorldModelVisibilityError,
    WorldModelVisibilityQuery, WorldModelVisibilityVisit, WorldSceneCameraFrame, WorldSceneFrustum,
};
