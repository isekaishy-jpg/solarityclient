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
    CameraSubjectHeightSource, PlayerCameraPose, PlayerCameraPoseError,
    resolve_camera_subject_height, resolve_model_camera_subject_height, resolve_player_camera_pose,
};
pub use character::{UnitModelAppearance, UnitModelAppearanceError, resolve_unit_model};
pub use equipment::{
    PlayerEquipmentAppearance, PlayerEquipmentAppearanceError, ResolvedEquipmentItem,
    resolve_player_equipment,
};
pub use object::{ObjectProjectionError, project_object_fields};
