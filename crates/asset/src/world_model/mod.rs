//! WMO root, group, material, doodad, and placement decoding.
//!
//! The boundary follows the stock `MapObj.cpp`, `MapObjGroup.cpp`, and
//! `MapObjRead.cpp` family while keeping renderer-owned scene resources out of
//! asset parsing.

mod map_obj;
mod map_obj_doodad;
mod map_obj_fog;
mod map_obj_group;
mod map_obj_read;
mod map_obj_spatial;

pub use map_obj::{DecodedWorldModel, WorldModelBlendMode, WorldModelMaterial, WorldModelShader};
pub use map_obj_doodad::{WorldModelDoodad, WorldModelDoodadSet, WorldModelDoodadSetError};
pub use map_obj_fog::{
    WorldModelFog, WorldModelFogBank, WorldModelFogPalette, sample_world_model_fog,
};
pub use map_obj_group::{
    DecodedWorldModelGroup, WorldModelBatch, WorldModelBatchClass, WorldModelBspNode,
    WorldModelLiquid, WorldModelLiquidVertex, WorldModelPolygon,
};
pub use map_obj_spatial::{WorldModelGroupInfo, WorldModelPortal, WorldModelPortalReference};
