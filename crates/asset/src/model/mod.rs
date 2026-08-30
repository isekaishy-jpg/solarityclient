//! M2 model parsing and CPU-side model representation.
//!
//! The stock `M2Model.cpp`, `M2Shared.cpp`, `ModelBlob.cpp`, and `M2Cache.cpp`
//! family supports a format boundary separate from GPU model resources.

mod m2_cache;
mod m2_model;
mod m2_shared;
mod model_blob;

pub use m2_model::DecodedM2Model;
pub use model_blob::{M2Batch, M2SkinProfile, M2Submesh, M2Vertex};
