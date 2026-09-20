//! Byte admission and typed retained storage; domains still own resource semantics.

mod budget;
mod buffer;
mod cell;
mod output;
mod result;
mod scratch;
mod types;
mod worker_scratch;

pub use budget::{ByteReservation, CpuStorageBudget};
pub(crate) use buffer::{StorageDeque, StorageVec};
pub use cell::CpuOwnedCell;
pub use output::{CpuBuffer, FixedWriter, OutputBuffer};
pub use result::{CpuResultLease, CpuResultPage};
pub use scratch::{CpuScratch, ScratchScope};
pub use types::{CpuStorageClass, CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot};
pub use worker_scratch::CpuWorkerScratch;
