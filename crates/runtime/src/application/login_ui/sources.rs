//! Required UI sources retain one worker reader and yield to shared BLP producers.

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetMount, AssetPath, AssetStore, BlpLoadDependency,
    BlpTextureCache, BlpTextureSource,
};
use solarity_cpu::{CpuError, CpuService, CpuTaskStep, JobContext};
use std::{ops::ControlFlow, sync::Arc};

use crate::application::{ApplicationError, texture_source_job::SharedTextureSources};

pub(super) struct UiTextureLoader {
    catalog: ArchiveCatalog,
    reader: Option<AssetStore>,
}

impl UiTextureLoader {
    pub(super) fn new(catalog: ArchiveCatalog) -> Self {
        Self {
            catalog,
            reader: None,
        }
    }

    /// Admission precedes moving the retained reader/cache. Only completed
    /// sources cross back to main; publication still follows authored order.
    pub(super) fn load(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        cache: &mut BlpTextureCache,
        paths: Vec<AssetPath>,
    ) -> Result<Vec<Arc<BlpTextureSource>>, ApplicationError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let cpu = renderer.executor();
        let permit = cpu.try_reserve_for(CpuService::Required)?;
        let shared = SharedTextureSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let task = permit.submit_resumable_with_context(prepare_sources(
            self.catalog.clone(),
            self.reader.take(),
            std::mem::take(cache),
            paths,
            shared,
        ));
        let prepared = renderer.join_source(task)?;
        self.reader = prepared.reader;
        *cache = prepared.cache;
        Ok(prepared.sources.map_err(solarity_ui::UiRenderError::from)?)
    }
}

/// Masks already occur in the deduplicated authored request order. Nonblocking
/// textures remain with the stock streaming owner, including missing sources.
pub(super) fn missing_paths(
    plan: &solarity_ui::UiTextureAssetPlan,
    resident: impl Fn(&AssetPath) -> bool,
) -> Vec<AssetPath> {
    plan.requests()
        .iter()
        .filter(|request| {
            request.residency() == solarity_rendering::UiTextureResidency::Blocking
                && !resident(request.path())
        })
        .map(|request| request.path().clone())
        .collect()
}

struct PreparedSources {
    reader: Option<AssetStore>,
    cache: BlpTextureCache,
    sources: Result<Vec<Arc<BlpTextureSource>>, AssetError>,
}

enum Reader {
    Unopened(ArchiveCatalog),
    Mounting(AssetMount),
    Ready(AssetStore),
}

struct SourceJob {
    reader: Option<Reader>,
    cache: BlpTextureCache,
    paths: std::vec::IntoIter<AssetPath>,
    sources: Vec<Arc<BlpTextureSource>>,
    pending: Option<BlpLoadDependency>,
    shared: SharedTextureSources,
}

fn prepare_sources(
    catalog: ArchiveCatalog,
    reader: Option<AssetStore>,
    cache: BlpTextureCache,
    paths: Vec<AssetPath>,
    shared: SharedTextureSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<PreparedSources> {
    let mut job = SourceJob {
        reader: Some(reader.map_or_else(|| Reader::Unopened(catalog), Reader::Ready)),
        cache,
        sources: Vec::with_capacity(paths.len()),
        paths: paths.into_iter(),
        pending: None,
        shared,
    };
    move |context| job.step(context)
}

impl SourceJob {
    fn finish(&mut self, result: Result<(), AssetError>) -> CpuTaskStep<PreparedSources> {
        self.pending = None;
        CpuTaskStep::Complete(PreparedSources {
            reader: match self.reader.take() {
                Some(Reader::Ready(reader)) => Some(reader),
                _ => None,
            },
            cache: std::mem::take(&mut self.cache),
            sources: result.map(|()| std::mem::take(&mut self.sources)),
        })
    }

    fn step(&mut self, context: &JobContext<'_>) -> CpuTaskStep<PreparedSources> {
        context.diagnostic_value("ui.texture.source_step", 1);
        if context.is_cancelled() {
            return self.finish(Err(CpuError::JobCancelled.into()));
        }
        let reader = self
            .reader
            .take()
            .unwrap_or_else(|| unreachable!("one UI source owner"));
        match reader {
            Reader::Unopened(catalog) => match AssetStore::begin_mount(catalog) {
                Ok(mount) => self.reader = Some(Reader::Mounting(mount)),
                Err(error) => return self.finish(Err(error)),
            },
            Reader::Mounting(mount) => match mount.advance() {
                Ok(ControlFlow::Continue(mount)) => self.reader = Some(Reader::Mounting(mount)),
                Ok(ControlFlow::Break(reader)) => self.reader = Some(Reader::Ready(reader)),
                Err(error) => return self.finish(Err(error)),
            },
            Reader::Ready(mut reader) => {
                let Some(path) = self.paths.as_slice().first() else {
                    self.reader = Some(Reader::Ready(reader));
                    return self.finish(Ok(()));
                };
                let source = match self.shared.load(&mut reader, path, &mut self.pending) {
                    Ok(ControlFlow::Continue(edge)) => {
                        self.reader = Some(Reader::Ready(reader));
                        return CpuTaskStep::Wait(edge);
                    }
                    Ok(ControlFlow::Break(source)) => reader
                        .with_read_budget(&self.shared.read_budget(), |reader| {
                            self.cache.adopt(reader, source)
                        }),
                    Err(error) => Err(error.into()),
                };
                self.reader = Some(Reader::Ready(reader));
                match source {
                    Ok(source) => self.sources.push(source),
                    Err(error) => return self.finish(Err(error)),
                }
                self.paths.next();
            }
        }
        CpuTaskStep::Continue
    }
}

#[cfg(test)]
#[path = "../../../tests/application/ui_texture_sources.rs"]
mod tests;
