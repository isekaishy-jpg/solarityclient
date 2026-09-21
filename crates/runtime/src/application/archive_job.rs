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

/// Connects domain-owned archive steps to the admitted job's trace and worker
/// boundary. Required cache/input return still completes after withdrawal.
pub(super) fn contextual<T>(
    name: &'static str,
    mut operation: impl FnMut() -> ControlFlow<T>,
) -> impl FnMut(&solarity_cpu::JobContext<'_>) -> ControlFlow<T> {
    let mut step = 0_u64;
    move |context| {
        context.diagnostic_value(name, step);
        step = step.saturating_add(1);
        operation()
    }
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
    let mut operation = prepare_archive_resumable(catalog, move |store| match prepare(store) {
        ControlFlow::Continue(()) => solarity_cpu::CpuTaskStep::Continue,
        ControlFlow::Break(result) => solarity_cpu::CpuTaskStep::Complete(result),
    });
    move || match operation() {
        solarity_cpu::CpuTaskStep::Continue => ControlFlow::Continue(()),
        solarity_cpu::CpuTaskStep::Complete(result) => ControlFlow::Break(result),
        solarity_cpu::CpuTaskStep::Wait(_) => {
            unreachable!("finite archive adapter never declares dependencies")
        }
    }
}

/// Retains the mounted reader across a domain's discovered readiness dependency.
/// The scheduler releases the worker; publication, cancellation and result policy
/// remain with the domain operation. No archive or cache lock crosses suspension.
pub(super) fn prepare_archive_resumable<T, E>(
    catalog: ArchiveCatalog,
    mut prepare: impl FnMut(&mut AssetStore) -> solarity_cpu::CpuTaskStep<Result<T, E>>,
) -> impl FnMut() -> solarity_cpu::CpuTaskStep<Result<T, E>>
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
                Err(error) => return solarity_cpu::CpuTaskStep::Complete(Err(error.into())),
            },
            Stage::Mounting(mount) => match mount.advance() {
                Ok(ControlFlow::Continue(mount)) => stage = Some(Stage::Mounting(mount)),
                Ok(ControlFlow::Break(store)) => stage = Some(Stage::Ready(store)),
                Err(error) => return solarity_cpu::CpuTaskStep::Complete(Err(error.into())),
            },
            Stage::Ready(mut store) => match prepare(&mut store) {
                solarity_cpu::CpuTaskStep::Continue => stage = Some(Stage::Ready(store)),
                solarity_cpu::CpuTaskStep::Wait(edge) => {
                    stage = Some(Stage::Ready(store));
                    return solarity_cpu::CpuTaskStep::Wait(edge);
                }
                solarity_cpu::CpuTaskStep::Complete(result) => {
                    return solarity_cpu::CpuTaskStep::Complete(result);
                }
            },
        }
        solarity_cpu::CpuTaskStep::Continue
    }
}
