//! Shared WMO scene admission, ordered graphics clips and unit destinations.

mod admission;
mod graphics;
mod outdoor;
mod sky;

pub(super) use admission::WorldSceneAdmission;
pub(in crate::application) use graphics::WorldModelSceneGroup;
