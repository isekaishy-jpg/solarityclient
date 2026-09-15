//! Shared backdrop assets decoded through one worker-private archive owner.

mod archive;
mod dispatch;
mod service;

#[cfg(test)]
#[path = "../../../../tests/application/backdrop_loader.rs"]
mod tests;

use super::RuntimeGlueModelError;
use crate::application::terrain_frame::m2::GlueM2Texture;
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetNamespaceId, AssetPath, AssetStore, BlpTextureCache,
    DecodedM2Model, M2CacheService, M2LoadProducer, M2LoadRequest, ResourceLease,
};
use solarity_cpu::CpuTask;
use std::{collections::HashMap, sync::Arc};

/// Immutable archive results shared by prewarm and selected-scene preparation.
pub(super) struct GlueBackdropAssets {
    pub(super) model: ResourceLease<DecodedM2Model>,
    pub(super) textures: Vec<GlueM2Texture>,
}

/// One exact result is retained, including failure, without repeated archive requests.
type BackdropResult = Result<Arc<GlueBackdropAssets>, Arc<RuntimeGlueModelError>>;

/// Mounted handles and decode caches are only accessed by the active CPU job.
struct BackdropArchiveState {
    store: AssetStore,
    textures: BlpTextureCache,
}

/// Retains the catalog across capacity refusal and mounts it once on a worker.
struct BackdropArchiveOwner {
    catalog: ArchiveCatalog,
    state: Option<Result<BackdropArchiveState, Arc<AssetError>>>,
}

/// Workers receive either useful decode work or an already published model, never a waiter.
enum BackdropModel {
    Ready(ResourceLease<DecodedM2Model>),
    Producer(M2LoadProducer),
}

/// Waiting for another producer retains only a request; no CPU slot or archive lock is held.
struct WaitingModel {
    path: AssetPath,
    request: M2LoadRequest,
}

/// Worker state returns with the domain outcome; no archive/cache guard spans decoding.
struct BackdropCompletion {
    assets: BackdropArchiveOwner,
    result: BackdropResult,
}

/// The scheduler never drops an active task when the requested scene changes.
struct PendingBackdrop {
    path: AssetPath,
    task: CpuTask<BackdropCompletion>,
    model_demand: Option<M2LoadRequest>,
}

/// Main-thread result cache with one finite, owned archive job at a time.
pub(super) struct GlueBackdropLoader {
    assets: Option<BackdropArchiveOwner>,
    sources: M2CacheService,
    namespace: AssetNamespaceId,
    waiting: Option<WaitingModel>,
    ready: HashMap<AssetPath, BackdropResult>,
    pending: Option<PendingBackdrop>,
}
