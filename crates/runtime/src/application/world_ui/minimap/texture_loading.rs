//! A mounted reader survives bounded service turns and admission backpressure.

use super::TextureCompletion;
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetMount, AssetPath, AssetStore, BlpTextureSource,
};
use solarity_cpu::JobContext;
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
) -> impl FnMut(&JobContext<'_>) -> ControlFlow<Result<TextureCompletion, AssetError>> {
    let mut stage = Some(match store {
        Some(store) => Stage::Ready(store),
        None => Stage::Unopened(catalog),
    });
    let mut paths = paths.into_iter();
    let mut sources = Vec::with_capacity(paths.len());
    move |context| {
        context.diagnostic_value("ui.minimap.remaining_sources", paths.len() as u64);
        let current = stage
            .take()
            .unwrap_or_else(|| unreachable!("terminal texture loading cannot resume"));
        match current {
            Stage::Unopened(catalog) => match AssetStore::begin_mount(catalog) {
                Ok(mount) => stage = Some(Stage::Mounting(mount)),
                Err(error) => return ControlFlow::Break(Err(error)),
            },
            Stage::Mounting(mount) => match mount.advance() {
                Ok(ControlFlow::Continue(mount)) => stage = Some(Stage::Mounting(mount)),
                Ok(ControlFlow::Break(store)) => stage = Some(Stage::Ready(store)),
                Err(error) => return ControlFlow::Break(Err(error)),
            },
            Stage::Ready(mut store) => {
                let Some(path) = paths.next() else {
                    return ControlFlow::Break(Ok(TextureCompletion {
                        store,
                        sources: std::mem::take(&mut sources),
                    }));
                };
                let source = BlpTextureSource::load(&mut store, &path);
                sources.push((path, source));
                stage = Some(Stage::Ready(store));
            }
        }
        ControlFlow::Continue(())
    }
}
