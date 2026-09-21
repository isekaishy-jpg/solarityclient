//! Shared primary-model ownership separates request metadata, decoding and consumption.

mod consumer;
mod producer;
mod readiness;
pub use readiness::M2LoadDependency;
mod service;

use super::super::resource::ResourceLease;
use super::{M2CacheService, ModelCacheCore};
use crate::{AssetError, AssetNamespaceId, AssetResourceKey, DecodedM2Model};
use solarity_cpu::CpuServiceInterest;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

/// Pipeline failure remains distinct from malformed or missing stock source data.
#[derive(Clone, Debug, Error)]
pub enum M2LoadError {
    /// Every joined consumer observes the same original archive/decode error.
    #[error(transparent)]
    Asset(Arc<AssetError>),
    /// A producer was given a reader for a different immutable archive selection.
    #[error("model request namespace {expected:?} differs from reader {actual:?}")]
    Namespace {
        /// Immutable archive selection captured by the request.
        expected: AssetNamespaceId,
        /// Archive selection of the supplied mounted reader.
        actual: AssetNamespaceId,
    },
    /// A panic or shutdown released the sole producer before it published a result.
    #[error("model request producer ended before publication")]
    Abandoned,
    /// CPU workers must return dependency ownership instead of parking for another producer.
    #[error("CPU worker cannot wait for unfinished model work")]
    WorkerWait,
}

/// One immutable outcome contains tracked external source ownership.
type Outcome = Result<ResourceLease<DecodedM2Model>, M2LoadError>;

/// M2 demand uses the common typed readiness bridge without sharing cache policy.
type Slot = super::super::source_dependency::SourceSlot<ResourceLease<DecodedM2Model>, M2LoadError>;

/// The table owns pending producers only; successful source retention uses the normal cache.
#[derive(Default)]
pub(super) struct RequestIndex {
    core: Option<Arc<ModelCacheCore>>,
    pending: HashMap<AssetResourceKey, Arc<Slot>>,
}

/// Admission distinguishes reuse, an existing producer and a new producer obligation.
pub enum M2Load {
    /// The qualified retained source is already available.
    Ready(ResourceLease<DecodedM2Model>),
    /// Another producer owns decoding; consumers poll or declare a readiness dependency.
    Pending(M2LoadRequest),
    /// This owner must publish decoding or explicitly abandon the request.
    Producer(M2LoadProducer),
}

/// A consumer can disappear independently without cancelling another owner's decode.
#[derive(Clone)]
pub struct M2LoadRequest {
    slot: Arc<Slot>,
    interest: CpuServiceInterest,
}

/// Unique decode ownership survives task transfer; drop publishes failure rather than stranding joiners.
pub struct M2LoadProducer {
    service: M2CacheService,
    key: AssetResourceKey,
    slot: Arc<Slot>,
    finished: bool,
}
