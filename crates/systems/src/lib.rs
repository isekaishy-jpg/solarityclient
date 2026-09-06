//! Domain systems that operate through the ECS boundary.

mod achievement;
mod arena;
mod auction;
mod battlefield;
mod calendar;
mod camera;
mod character;
mod collision;
mod combat;
mod currency;
mod dance;
mod duel;
mod effect;
mod equipment;
mod group_finder;
mod guild;
mod interaction;
mod inventory;
mod language;
mod loot;
mod mail;
mod missile;
mod movement;
mod name_cache;
mod object;
mod pet;
mod petition;
mod player;
mod quest;
mod raid;
mod reputation;
mod skill;
mod social;
mod spell;
mod stable;
mod support;
mod talent;
mod vehicle;
mod world;

pub use camera::{
    CameraSubjectGeometry, CameraSubjectHeight, CameraSubjectHeightError,
    CameraSubjectHeightSource, MountCameraGeometry, MountCameraHeightError,
    PlayerCameraHeightSample, PlayerCameraHeightState, PlayerCameraObstructionError,
    PlayerCameraPose, PlayerCameraPoseError, PlayerCameraWaterError, resolve_camera_subject_height,
    resolve_model_camera_subject_height, resolve_mounted_player_camera_pose,
    resolve_player_camera_obstruction, resolve_player_camera_pose,
    resolve_player_camera_water_collision,
};
pub use character::{UnitModelAppearance, UnitModelAppearanceError, resolve_unit_model};
pub use collision::{
    M2CollisionError, M2CollisionScene, MovementBspCacheMode, MovementCollectionError,
    MovementCollisionBounds, MovementCollisionPlane, MovementCollisionTriangle,
    MovementCollisionVolume, MovementFallContactKind, MovementSupportProfile, MovementSweep,
    MovementSweepError, MovementTerrainChunks, PlacedM2Collision, PlacedWorldModelCollision,
    PlacedWorldModelLiquid, TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh,
    TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample, TerrainRegistrationPoint,
    WorldModelCollisionError, WorldModelCollisionScene, WorldModelFloorHit, WorldModelFloorHits,
    WorldModelLiquidError, WorldModelLiquidSample, WorldModelLiquidScene, WorldModelPortalHit,
    WorldModelRegistrationCandidate, WorldModelRegistrationHit, WorldModelRegistrationHits,
    WorldModelRegistrationKind, WorldModelRegistrationQuery, WorldModelRegistrationSelection,
    probe_world_model_portals,
};
pub use equipment::{
    PlayerEquipmentAppearance, PlayerEquipmentAppearanceError, ResolvedEquipmentItem,
    resolve_player_equipment,
};
pub use movement::{
    MovementFallAdmission, MovementFallAdvance, MovementFallAdvanceError,
    MovementFallAdvancePolicy, MovementFallContact, MovementFallContactError,
    MovementFallContactQuery, MovementFallContinuation, MovementFallCrossing, MovementFallError,
    MovementFallInterval, MovementFallMode, MovementFallPhase, MovementFallSnapshot,
    MovementFallState, MovementFallTrajectory, MovementGroundAdvance, MovementGroundAdvanceError,
    MovementGroundContinuation, MovementGroundInterval, MovementGroundProfile,
    MovementGroundSnapshot, MovementGroundState, MovementIntervalBounds,
    MovementIntervalBoundsError, MovementIntervalMode, MovementIntervalRequest,
    UnitLocomotionAnimation, UnitModelAnimation, WorldEntryGroundContact,
    WorldEntryGroundContactError, resolve_unit_locomotion_animation, resolve_unit_model_animation,
};
pub use object::{
    GameObjectAnimationRequest, GameObjectAnimationState, GameObjectPlacement,
    GameObjectPlacementError, GameObjectPlacementResolver, ObjectProjectionError,
    game_object_reversed_progress, game_object_sequence_offset, project_object_fields,
    unpack_game_object_rotation,
};
pub use world::{
    DEFAULT_WORLD_VIEW_DISTANCE, EXTENDED_WORLD_VIEW_DISTANCE_MAXIMUM,
    LEGACY_WORLD_VIEW_DISTANCE_MAXIMUM, TerrainStreamingError, TerrainStreamingWindow,
    WORLD_VIEW_DISTANCE_MINIMUM, WorldViewDistance, WorldViewDistanceError, WorldViewDistanceLimit,
    WorldViewDistanceRequest, prioritize_terrain_tiles, resolve_world_view_distance,
};
