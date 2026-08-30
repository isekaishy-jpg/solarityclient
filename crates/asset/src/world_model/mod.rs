//! WMO root, group, material, doodad, and placement decoding.
//!
//! The boundary follows the stock `MapObj.cpp`, `MapObjGroup.cpp`, and
//! `MapObjRead.cpp` family while keeping renderer-owned scene resources out of
//! asset parsing.

mod map_obj;
mod map_obj_group;
mod map_obj_read;

pub use map_obj::DecodedWorldModel;
pub use map_obj_group::{DecodedWorldModelGroup, WorldModelBspNode, WorldModelPolygon};
