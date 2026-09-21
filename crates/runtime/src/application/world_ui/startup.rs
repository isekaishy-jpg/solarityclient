//! Finite archive preparation overlapped with covered world loading.

#[cfg(test)]
#[path = "../../../tests/application/world_ui_sources.rs"]
mod source_preparation_tests;

use std::ops::ControlFlow;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, MapDifficultyCatalog, MinimapTextureCatalog,
    SpellNameCatalog,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_ui::{FrameUiSources, GlueError};

use crate::application::{ApplicationError, archive_job::prepare_archive};

/// Read-only world UI inputs with no character, Lua or renderer ownership.
/// A completed image is reusable after a cancelled entry because the mounted
/// archive catalog is fixed for the client lifetime. At most one image is held.
pub(in crate::application) struct WorldUiSourceImage {
    pub(super) spell_names: SpellNameCatalog,
    pub(super) frame: FrameUiSources,
    pub(super) minimap: Result<MinimapTextureCatalog, AssetError>,
    pub(super) transfer_messages: TransferMessages,
}

/// An optional packet path observes its exact prepared error only when requested.
pub(super) struct TransferMessages(Result<MapDifficultyCatalog, std::sync::Arc<AssetError>>);

impl TransferMessages {
    pub(super) fn message(
        &self,
        map_id: u32,
        reason: u8,
        argument: Option<u8>,
    ) -> Result<Option<&str>, super::RuntimeWorldUiError> {
        let (8, Some(difficulty)) = (reason, argument) else {
            return Ok(None);
        };
        let catalog = self.0.as_ref().map_err(|error| {
            super::RuntimeWorldUiError::TransferMetadata(std::sync::Arc::clone(error))
        })?;
        Ok(catalog.message(map_id, u32::from(difficulty)))
    }
}

impl WorldUiSourceImage {
    /// Preserves the synchronous entry path's first-error order.
    pub(in crate::application) fn load(store: &mut AssetStore) -> Result<Self, GlueError> {
        let _profile = solarity_profiling::profile!("runtime.world_ui.source_preparation");
        let spell_names = SpellNameCatalog::load(store)?;
        let frame = FrameUiSources::load(store)?;
        let minimap = MinimapTextureCatalog::load(store);
        let transfer_messages =
            TransferMessages(MapDifficultyCatalog::load(store).map_err(std::sync::Arc::new));
        Ok(Self {
            spell_names,
            frame,
            minimap,
            transfer_messages,
        })
    }
}

/// Owns one loader result until entry consumes it or shutdown observes it.
#[derive(Default)]
pub(in crate::application) struct WorldUiSourcePreparation {
    task: Option<CpuTask<Result<WorldUiSourceImage, GlueError>>>,
    ready: Option<WorldUiSourceImage>,
}

impl WorldUiSourcePreparation {
    /// Polls without joining unfinished work or occupying the frame executor.
    pub(in crate::application) fn poll(&mut self) -> Result<(), ApplicationError> {
        if self.task.as_ref().is_some_and(CpuTask::is_finished) {
            self.finish()?;
        }
        Ok(())
    }

    /// Applies bounded-pool backpressure without falling back to main-thread I/O.
    pub(in crate::application) fn request(
        &mut self,
        cpu: &CpuExecutor,
        catalog: &ArchiveCatalog,
    ) -> Result<(), ApplicationError> {
        if self.task.is_some() || self.ready.is_some() {
            return Ok(());
        }
        let permit = match cpu.try_reserve() {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let catalog = catalog.clone();
        let budget = solarity_asset::AssetReadBudget::for_service(
            cpu.storage().clone(),
            solarity_cpu::CpuService::Required,
        );
        self.task = Some(permit.submit_steps_with_context(
            crate::application::archive_job::contextual(
                "world_ui.source_step",
                prepare_archive(catalog, move |store| {
                    ControlFlow::Break(store.with_read_budget(&budget, WorldUiSourceImage::load))
                }),
            ),
        ));
        Ok(())
    }

    /// Transfers declarations only when the caller has authoritative world facts.
    pub(in crate::application) fn take(&mut self) -> Option<WorldUiSourceImage> {
        self.ready.take()
    }

    /// Observes an admitted result even if world entry was cancelled. Normal
    /// polling calls this only after completion; shutdown may wait for the job.
    pub(in crate::application) fn finish(&mut self) -> Result<(), ApplicationError> {
        if let Some(task) = self.task.take() {
            self.ready = Some(task.join()??);
        }
        Ok(())
    }
}
