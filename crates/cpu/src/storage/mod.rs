//! Byte admission and typed retained storage; domains still own resource semantics.

mod budget;
mod buffer;
mod result;
mod types;

pub use budget::{ByteReservation, CpuStorageBudget};
pub(crate) use buffer::{StorageDeque, StorageVec};
pub use result::{CpuResultLease, CpuResultPage};
pub use types::{CpuStorageClass, CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot};
