//! Stock effect images and the native text overlay prepare during Vulkan startup.

use crate::application::texture_source_job::SharedTextureSources;
use solarity_asset::{AssetError, AssetPath, BlpLoadError, BlpTextureSource};
use solarity_cpu::CpuTaskStep;
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

/// Retains fixed source order and a typed BLP dependency on the startup reader.
pub(super) struct Preparation {
    sources: EffectTextures,
    index: usize,
    pending: Option<solarity_asset::BlpLoadDependency>,
    shared: SharedTextureSources,
    pixel_extent: (u32, u32),
}

impl Preparation {
    pub(super) fn new(shared: SharedTextureSources, pixel_extent: (u32, u32)) -> Self {
        Self {
            sources: [None, None, None],
            index: 0,
            pending: None,
            shared,
            pixel_extent,
        }
    }

    /// One authored texture per turn. Missing/invalid stock textures retain their
    /// slots; infrastructure failures stay fatal. Font rasterization is indivisible.
    pub(super) fn step(
        &mut self,
        store: &mut solarity_asset::AssetStore,
    ) -> CpuTaskStep<Result<PreparedStartupPresentation, FontError>> {
        let Some(path) = PATHS.get(self.index) else {
            let result = store.with_read_budget(&self.shared.read_budget(), |store| {
                crate::application::performance_overlay::PreparedFpsOverlay::load(
                    store,
                    self.pixel_extent,
                )
            });
            return CpuTaskStep::Complete(result.map(|fps| PreparedStartupPresentation {
                effects: std::mem::take(&mut self.sources),
                fps,
            }));
        };
        let path = match AssetPath::new(path) {
            Ok(path) => path,
            Err(error) => return CpuTaskStep::Complete(Err(error.into())),
        };
        match self.shared.load(store, &path, &mut self.pending) {
            Ok(ControlFlow::Continue(edge)) => return CpuTaskStep::Wait(edge),
            Ok(ControlFlow::Break(source)) => self.sources[self.index] = Some(Arc::new(source)),
            Err(BlpLoadError::Asset(error)) if !error.is_source_pipeline_error() => {
                tracing::warn!(texture = %path, %error, "fixed effect texture request failed; using stock green texture");
            }
            Err(error) => return CpuTaskStep::Complete(Err(AssetError::from(error).into())),
        }
        self.index += 1;
        CpuTaskStep::Continue
    }
}

/// Focused fixtures exercise the same presentation stages without startup catalogs.
#[cfg(test)]
pub(super) fn prepare(
    catalog: solarity_asset::ArchiveCatalog,
    shared: SharedTextureSources,
    pixel_extent: (u32, u32),
) -> impl FnMut(
    &solarity_cpu::JobContext<'_>,
) -> CpuTaskStep<Result<PreparedStartupPresentation, FontError>> {
    let mut preparation = Preparation::new(shared, pixel_extent);
    let mut operation =
        crate::application::archive_job::prepare_archive_resumable(catalog, move |store| {
            preparation.step(store)
        });
    move |context| {
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(AssetError::from(
                solarity_cpu::CpuError::JobCancelled,
            )
            .into()));
        }
        operation()
    }
}

#[cfg(test)]
#[path = "../../../tests/application/startup_presentation.rs"]
mod tests;
