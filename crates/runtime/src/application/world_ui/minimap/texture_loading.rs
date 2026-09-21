//! A mounted reader survives bounded service turns and admission backpressure.

use super::TextureCompletion;
use crate::application::texture_source_job::SharedTextureSources;
use solarity_asset::{ArchiveCatalog, AssetError, AssetMount, AssetPath, AssetStore};
use solarity_cpu::{CpuTaskStep, JobContext};
use std::ops::ControlFlow;

/// Partial archive stacks never escape into texture preparation.
enum Stage {
    Unopened(ArchiveCatalog),
    Mounting(AssetMount),
    Ready(AssetStore),
}

/// Owns all inputs after a task permit has been reserved. Each turn opens one
/// archive or decodes one texture; errors retain the original per-path policy.
pub(super) fn prepare(
    catalog: ArchiveCatalog,
    store: Option<AssetStore>,
    paths: Vec<AssetPath>,
    shared: SharedTextureSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<TextureCompletion, AssetError>> {
    let mut stage = Some(match store {
        Some(store) => Stage::Ready(store),
        None => Stage::Unopened(catalog),
    });
    let mut paths = paths.into_iter();
    let mut pending = None;
    let mut sources = Vec::with_capacity(paths.len());
    move |context| {
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(solarity_cpu::CpuError::JobCancelled.into()));
        }
        context.diagnostic_value("ui.minimap.remaining_sources", paths.len() as u64);
        let current = stage
            .take()
            .unwrap_or_else(|| unreachable!("terminal texture loading cannot resume"));
        match current {
            Stage::Unopened(catalog) => match AssetStore::begin_mount(catalog) {
                Ok(mount) => stage = Some(Stage::Mounting(mount)),
                Err(error) => return CpuTaskStep::Complete(Err(error)),
            },
            Stage::Mounting(mount) => match mount.advance() {
                Ok(ControlFlow::Continue(mount)) => stage = Some(Stage::Mounting(mount)),
                Ok(ControlFlow::Break(store)) => stage = Some(Stage::Ready(store)),
                Err(error) => return CpuTaskStep::Complete(Err(error)),
            },
            Stage::Ready(mut store) => {
                let Some(path) = paths.as_slice().first().cloned() else {
                    return CpuTaskStep::Complete(Ok(TextureCompletion {
                        store,
                        sources: std::mem::take(&mut sources),
                    }));
                };
                let source = match shared.load(&mut store, &path, &mut pending) {
                    Ok(ControlFlow::Continue(edge)) => {
                        stage = Some(Stage::Ready(store));
                        return CpuTaskStep::Wait(edge);
                    }
                    Ok(ControlFlow::Break(source)) => Ok(source),
                    Err(error) => Err(error),
                };
                paths.next();
                sources.push((path, source));
                stage = Some(Stage::Ready(store));
            }
        }
        CpuTaskStep::Continue
    }
}
