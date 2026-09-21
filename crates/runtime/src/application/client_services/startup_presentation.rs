//! Stock effect images and the native text overlay prepare during Vulkan startup.

use crate::application::{
    archive_job::prepare_archive_resumable, texture_source_job::SharedTextureSources,
};
use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, BlpLoadError, BlpTextureSource};
use solarity_cpu::{CpuError, CpuTaskStep, JobContext};
use solarity_ui::FontError;
use std::{ops::ControlFlow, sync::Arc};

const PATHS: [&str; 3] = [
    "XTextures/splash/splash.blp",
    "XTextures/splash/wake.blp",
    "Textures/WaterPoop02.blp",
];

type EffectTextures = [Option<Arc<BlpTextureSource>>; 3];

pub(super) struct PreparedStartupPresentation {
    pub(super) effects: EffectTextures,
    pub(super) fps: Option<crate::application::performance_overlay::PreparedFpsOverlay>,
}

/// One archive or authored texture per turn. A missing/invalid stock texture
/// keeps its original failure-image slot; infrastructure failures remain fatal.
pub(super) fn prepare(
    catalog: ArchiveCatalog,
    shared: SharedTextureSources,
    pixel_extent: (u32, u32),
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<PreparedStartupPresentation, FontError>> {
    let mut sources: EffectTextures = [None, None, None];
    let mut index = 0;
    let mut pending = None;
    let mut operation = prepare_archive_resumable(catalog, move |store| {
        let Some(path) = PATHS.get(index) else {
            let result = store.with_read_budget(&shared.read_budget(), |store| {
                crate::application::performance_overlay::PreparedFpsOverlay::load(
                    store,
                    pixel_extent,
                )
            });
            return CpuTaskStep::Complete(result.map(|fps| PreparedStartupPresentation {
                effects: std::mem::take(&mut sources),
                fps,
            }));
        };
        let path = match AssetPath::new(path) {
            Ok(path) => path,
            Err(error) => return CpuTaskStep::Complete(Err(error.into())),
        };
        match shared.load(store, &path, &mut pending) {
            Ok(ControlFlow::Continue(edge)) => return CpuTaskStep::Wait(edge),
            Ok(ControlFlow::Break(source)) => sources[index] = Some(Arc::new(source)),
            Err(BlpLoadError::Asset(error)) if !error.is_source_pipeline_error() => {
                tracing::warn!(texture = %path, %error, "fixed effect texture request failed; using stock green texture");
            }
            Err(error) => return CpuTaskStep::Complete(Err(AssetError::from(error).into())),
        }
        index += 1;
        CpuTaskStep::Continue
    });
    move |context| {
        context.diagnostic_value("startup.presentation.source_step", 1);
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(AssetError::from(CpuError::JobCancelled).into()));
        }
        operation()
    }
}

#[cfg(test)]
#[path = "../../../tests/application/startup_presentation.rs"]
mod tests;
