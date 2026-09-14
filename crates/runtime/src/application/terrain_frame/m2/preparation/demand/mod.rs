//! CPU consumers request named bones independently of render palette admission.

mod bones;
mod model;

pub(in crate::application::terrain_frame::m2) use bones::CpuBoneDemand;
pub(in crate::application::terrain_frame::m2) use model::CpuModelInputs;
