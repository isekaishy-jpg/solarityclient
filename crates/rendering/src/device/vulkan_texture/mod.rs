//! Device-local authored BLP mip images and renderer-local identities.

mod registry;
mod status;
mod types;
mod upload;

pub(in crate::device) use registry::BlpTextureRegistry;
pub use status::BlpTextureUploadError;
pub use types::{
    BlpColorSpace, BlpTextureHandle, BlpTextureResourceInfo, BlpTextureSourceKind,
    BlpTextureStorage, BlpTextureUploadRequest,
};
pub(in crate::device) use upload::{GpuSampledImage, TextureUploadContext, upload_rgba8_image};
