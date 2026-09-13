//! Shared WMO scene admission, ordered graphics clips and unit destinations.

mod admission;
mod exterior;
mod graphics;
mod outdoor;
mod sky;

pub(super) use admission::WorldSceneAdmission;
pub(in crate::application) use graphics::WorldModelSceneGroup;
