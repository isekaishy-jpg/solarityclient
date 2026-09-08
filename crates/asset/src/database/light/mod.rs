//! Build-12340 outdoor light tables, cyclic band sampling, and volume blending.
//!
//! Light state is asset-derived and shared by terrain, fog, sky, water, WMO,
//! and M2 presentation. The catalog retains all recovered color and float
//! channels so individual renderers do not decode or reinterpret DBC rows.

mod catalog;
mod darkening;
mod direction;
mod fog;
mod sampling;
mod status;
mod types;

pub use catalog::LightCatalog;
pub use direction::{exterior_light_direction, exterior_light_direction_at, exterior_light_ray_at};
pub use fog::{WorldFogContext, WorldFogSample};
pub use status::WorldLightSampleError;
pub use types::{
    LightDefinition, LightParameter, LightSkybox, ModelLightColors, SkyboxBlend,
    WorldLightCondition, WorldLightQuery, WorldLightSample,
};
