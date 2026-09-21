//! Archive readers and source preparation live only in admitted bulk turns.

use super::*;
use crate::application::terrain_coordinator::{RuntimeTerrainError, SharedTerrainSources};
use solarity_asset::{ArchiveCatalog, M2LoadError};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask, CpuTaskStep, JobContext};
use solarity_rendering::M2LocalLightCount;
use std::ops::ControlFlow;

type PreparedModel = (ResidentM2Source, M2LocalLightCount);
type ModelTask = CpuTask<Result<PreparedModel, RuntimeTerrainError>>;
type CelestialSources = [Option<BlpTextureSource>; 5];

/// A retained sky owner retries admission, never decoding during presentation.
pub(super) struct ModelRequest {
    path: AssetPath,
    created_tick: u32,
    authored_lights: bool,
    task: Option<ModelTask>,
    complete: bool,
}

impl ModelRequest {
    pub(super) fn new(path: AssetPath, created_tick: u32, authored_lights: bool) -> Self {
        Self {
            path,
            created_tick,
            authored_lights,
            task: None,
            complete: false,
        }
    }

    pub(super) fn service(
        &mut self,
        cpu: &CpuExecutor,
        catalog: &ArchiveCatalog,
    ) -> Result<Option<SkyM2Model>, RuntimeTerrainFrameError> {
        if self.complete {
            return Ok(None);
        }
        if let Some(task) = &self.task {
            if !task.is_finished() {
                return Ok(None);
            }
            let result = self
                .task
                .take()
                .unwrap_or_else(|| unreachable!("finished task retained"))
                .join()?;
            self.complete = true;
            return match result {
                Ok((source, lights)) => Ok(Some(if self.authored_lights {
                    SkyM2Model::with_lights(source, self.created_tick, lights)
                } else {
                    SkyM2Model::new(source, self.created_tick)
                })),
                Err(RuntimeTerrainError::Cpu(error)) => Err(error.into()),
                Err(RuntimeTerrainError::SharedModel(error))
                    if !matches!(error, M2LoadError::Asset(_)) =>
                {
                    Err(error.into())
                }
                Err(error) => {
                    tracing::warn!(model = %self.path, %error, "authored sky model request failed");
                    Ok(None)
                }
            };
        }
        let permit = match cpu.try_reserve() {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let shared = SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        self.task = Some(permit.submit_resumable_with_context(model_steps(
            catalog.clone(),
            self.path.clone(),
            self.authored_lights,
            shared,
        )));
        Ok(None)
    }
}

/// Shared source readiness precedes consumer textures and shader preparation.
fn model_steps(
    catalog: ArchiveCatalog,
    path: AssetPath,
    authored_lights: bool,
    shared: SharedTerrainSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<PreparedModel, RuntimeTerrainError>> + Send {
    let mut pending = None;
    let mut model: Option<solarity_asset::ResourceLease<solarity_asset::DecodedM2Model>> = None;
    let mut textures = BlpTextureCache::new();
    let mut operation =
        crate::application::archive_job::prepare_archive_resumable(catalog, move |store| {
            if let Some(model) = model.take() {
                let lights = if authored_lights {
                    light_count(&model)
                } else {
                    M2LocalLightCount::Zero
                };
                return CpuTaskStep::Complete(
                    ResidentM2Source::from_model_with_lights(model, &mut textures, store, lights)
                        .map(|source| (source, lights)),
                );
            }
            match shared.model(&path, &mut pending, store) {
                Ok(ControlFlow::Break(source)) => {
                    model = Some(source);
                    CpuTaskStep::Continue
                }
                Ok(ControlFlow::Continue(edge)) => CpuTaskStep::Wait(edge),
                Err(error) => CpuTaskStep::Complete(Err(error)),
            }
        });
    move |context| {
        context.diagnostic_value("sky.source_step", 1);
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(CpuError::JobCancelled.into()));
        }
        operation()
    }
}

fn light_count(model: &solarity_asset::DecodedM2Model) -> M2LocalLightCount {
    let lights = model.animations().lights();
    let directional = lights
        .iter()
        .any(|light| light.kind() == solarity_asset::M2LightKind::Directional);
    let points = lights
        .iter()
        .filter(|light| light.kind() == solarity_asset::M2LightKind::Point)
        .count();
    match (usize::from(directional) + points).min(4) {
        0 => M2LocalLightCount::Zero,
        1 => M2LocalLightCount::One,
        2 => M2LocalLightCount::Two,
        3 => M2LocalLightCount::Three,
        _ => M2LocalLightCount::Four,
    }
}

pub(super) enum CelestialRequest {
    Deferred,
    Running(CpuTask<Result<CelestialSources, AssetError>>),
    Complete,
}

impl RuntimeSkyResources {
    /// One texture per turn; publication replaces any initial failure handles.
    pub(in crate::application) fn service_textures(
        &mut self,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        match &self.celestial_request {
            CelestialRequest::Deferred => {
                let permit = match cpu.try_reserve() {
                    Ok(permit) => permit,
                    Err(CpuError::AtCapacity { .. }) => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                let mut sources: CelestialSources = Default::default();
                let mut next = 0;
                let operation = crate::application::archive_job::prepare_archive(
                    self.catalog.clone(),
                    move |store| {
                        let Some(name) = [
                            "Textures/sunCenter.blp",
                            "Textures/moon.blp",
                            "Textures/moon02.blp",
                            "Textures/sunGlare.blp",
                            "Textures/moonGlare.blp",
                        ]
                        .get(next)
                        .copied() else {
                            return ControlFlow::Break(Ok(std::mem::take(&mut sources)));
                        };
                        let path = match AssetPath::new(name) {
                            Ok(path) => path,
                            Err(error) => return ControlFlow::Break(Err(error)),
                        };
                        match BlpTextureSource::load(store, &path) {
                            Ok(source) => sources[next] = Some(source),
                            Err(error) => {
                                tracing::warn!(texture = %path, %error, "celestial texture request failed; using stock green texture")
                            }
                        }
                        next += 1;
                        ControlFlow::Continue(())
                    },
                );
                self.celestial_request =
                    CelestialRequest::Running(permit.submit_steps_with_context(
                        crate::application::archive_job::contextual("sky.texture_step", operation),
                    ));
            }
            CelestialRequest::Running(task) if task.is_finished() => {
                let CelestialRequest::Running(task) =
                    std::mem::replace(&mut self.celestial_request, CelestialRequest::Complete)
                else {
                    unreachable!("finished celestial task retained")
                };
                match task.join()? {
                    Ok(sources) => {
                        self.sources = sources;
                        self.textures = [None; 5];
                    }
                    Err(error) => {
                        tracing::warn!(%error, "celestial archive request failed; using stock green textures")
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::application) fn service_models(
        &mut self,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if let Some(model) = self.stars_request.service(cpu, &self.catalog)? {
            self.stars = Some(model);
        }
        for entry in &mut self.skyboxes {
            if let Some(model) = entry.request.service(cpu, &self.catalog)? {
                entry.model = Some(model);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/application/sky_source_requests.rs"]
mod tests;
