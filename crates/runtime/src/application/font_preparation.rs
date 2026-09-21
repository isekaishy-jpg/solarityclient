//! Main-owned font host; required workers own every archive read and FreeType call.

use crate::platform::{NativeInputHandle, PlatformError};
use solarity_asset::{ArchiveCatalog, AssetError, AssetReadBudget, AssetStore};
use solarity_cpu::{CpuError, CpuExecutor, CpuService, CpuServiceHandle, CpuTask};
use solarity_ui::{FontError, FontSystem, FontWork, FontWorkExecutor, FontWorkOutput};
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

struct RuntimeFontPreparation {
    cpu: CpuServiceHandle,
    input: Option<NativeInputHandle>,
    catalog: ArchiveCatalog,
    reader: Arc<Mutex<Option<AssetStore>>>,
}

pub(super) fn font_system(
    cpu: &CpuExecutor,
    input: NativeInputHandle,
    catalog: ArchiveCatalog,
) -> FontSystem {
    FontSystem::with_executor(Rc::new(RuntimeFontPreparation {
        cpu: cpu.service_handle(),
        input: Some(input),
        catalog,
        reader: Arc::new(Mutex::new(None)),
    }))
}

impl FontWorkExecutor for RuntimeFontPreparation {
    fn execute(&self, work: FontWork) -> Result<FontWorkOutput, FontError> {
        let task = self
            .cpu
            .try_submit_prepared(|| {
                let catalog = self.catalog.clone();
                let reader = Arc::clone(&self.reader);
                let budget =
                    AssetReadBudget::for_service(self.cpu.storage().clone(), CpuService::Required);
                move |context: &solarity_cpu::JobContext<'_>| {
                    context.diagnostic_value("ui.font.required_request", 1);
                    if context.is_cancelled() {
                        return Err(AssetError::from(CpuError::JobCancelled).into());
                    }
                    let mut reader = reader.lock().map_err(|_| FontError::Execution {
                        message: "font archive reader is unavailable".into(),
                    })?;
                    if reader.is_none() {
                        *reader = Some(AssetStore::mount(catalog)?);
                    }
                    reader
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("font reader initialized"))
                        .with_read_budget(&budget, |store| work.run(store))
                }
            })
            .map_err(AssetError::from)?;
        let pending = Pending(Some(task));
        let waited = self.input.as_ref().map_or(Ok(()), |input| {
            input.wait_until_ready(|| Ok::<_, PlatformError>(pending.task().is_finished()))
        });
        if waited.is_err() {
            pending.task().cancel();
        }
        let result = pending.join();
        waited.map_err(|error| FontError::Execution {
            message: error.to_string(),
        })?;
        result.map_err(AssetError::from)?
    }
}

/// Native servicing failure or unwind cannot abandon a worker holding the cache.
struct Pending(Option<CpuTask<Result<FontWorkOutput, FontError>>>);
impl Pending {
    fn task(&self) -> &CpuTask<Result<FontWorkOutput, FontError>> {
        self.0
            .as_ref()
            .unwrap_or_else(|| unreachable!("pending font task"))
    }
    fn join(mut self) -> Result<Result<FontWorkOutput, FontError>, CpuError> {
        self.0
            .take()
            .unwrap_or_else(|| unreachable!("pending font task"))
            .join()
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.cancel();
            let _ = task.join();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/application/font_preparation.rs"]
mod tests;
