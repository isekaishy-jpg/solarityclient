//! Byte admission and typed retained storage; domains still own resource semantics.

mod budget;
mod buffer;
mod output;
mod result;
mod types;

pub use budget::{ByteReservation, CpuStorageBudget};
pub(crate) use buffer::{StorageDeque, StorageVec};
pub use output::{CpuBuffer, FixedWriter, OutputBuffer};
pub use result::{CpuResultLease, CpuResultPage};
pub use types::{CpuStorageClass, CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot};
