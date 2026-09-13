//! Immutable CPU preparation and renderer resource admission have separate owners.

mod cpu;
mod gpu;
pub(in crate::application) use cpu::{
    M2CpuSource, M2GlueCpuSourceKey, M2GluePipelineWarmup, prepare_m2_cpu_source,
};
pub(super) use gpu::{
    prepare_gpu_source, prepare_gpu_source_from_cpu, prepare_source, prepare_source_with_lights,
};
