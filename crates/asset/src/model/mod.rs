//! M2 model parsing and CPU-side model representation.
//!
//! The stock `M2Model.cpp`, `M2Shared.cpp`, `ModelBlob.cpp`, and `M2Cache.cpp`
//! family supports a format boundary separate from GPU model resources.

mod animation;
mod collision;
mod lookups;
mod m2_cache;
mod m2_model;
mod m2_shared;
mod model_blob;
mod skin_profile;

pub use animation::{
    M2AnimationSet, M2Attachment, M2Bone, M2Camera, M2ColorAnimation, M2Event, M2EventTrack,
    M2Interpolation, M2Light, M2LightKind, M2ParticleEmitter, M2ParticleLifetimeTrack,
    M2RibbonEmitter, M2Sequence, M2SequenceStorage, M2TextureTransform, M2TextureWeight, M2Track,
    M2TrackChannel,
};
pub use collision::M2CollisionMesh;
pub use m2_model::DecodedM2Model;
pub(crate) use m2_shared::canonical_model_path;
pub use model_blob::{
    M2BlendMode, M2HardcodedTextureSource, M2Material, M2ModelBounds, M2Texture, M2TextureKind,
    M2Vertex,
};
pub use skin_profile::{M2Batch, M2SkinProfile, M2Submesh};
