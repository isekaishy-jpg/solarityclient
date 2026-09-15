//! Finite archive-stack opening followed by domain-owned preparation steps.

use std::ops::ControlFlow;

use solarity_asset::{ArchiveCatalog, AssetError, AssetMount, AssetStore};

#[cfg(test)]
#[path = "../../tests/application/archive_job.rs"]
mod tests;

/// A partial stack is never offered to domain loading or main-thread consumers.
enum Stage {
    Unopened(ArchiveCatalog),
    Mounting(AssetMount),
    Ready(AssetStore),
}

/// Creates one resumable CPU operation. Admission must precede transferring the
/// catalog and domain captures here. Mounting opens one archive per turn, then
/// yields before domain preparation. The domain defines its own finite steps;
/// errors retire the partial stack on the worker without retries or publication.
pub(super) fn prepare_archive<T, E>(
    catalog: ArchiveCatalog,
    mut prepare: impl FnMut(&mut AssetStore) -> ControlFlow<Result<T, E>>,
) -> impl FnMut() -> ControlFlow<Result<T, E>>
where
    E: From<AssetError>,
{
    let mut stage = Some(Stage::Unopened(catalog));
    move || {
        let current = stage
            .take()
            .unwrap_or_else(|| unreachable!("a terminal archive operation cannot resume"));
        match current {
            Stage::Unopened(catalog) => match AssetStore::begin_mount(catalog) {
                Ok(mount) => stage = Some(Stage::Mounting(mount)),
                Err(error) => return ControlFlow::Break(Err(error.into())),
            },
            Stage::Mounting(mount) => match mount.advance() {
                Ok(ControlFlow::Continue(mount)) => stage = Some(Stage::Mounting(mount)),
                Ok(ControlFlow::Break(store)) => stage = Some(Stage::Ready(store)),
                Err(error) => return ControlFlow::Break(Err(error.into())),
            },
            Stage::Ready(mut store) => match prepare(&mut store) {
                ControlFlow::Continue(()) => stage = Some(Stage::Ready(store)),
                ControlFlow::Break(result) => return ControlFlow::Break(result),
            },
        }
        ControlFlow::Continue(())
    }
}
