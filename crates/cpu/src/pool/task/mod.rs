//! Service demand, cooperative control and typed completion ownership.

mod completion;
mod context;
mod service;
mod step;

pub use completion::CpuTask;
pub(super) use completion::TaskOutcome;
pub(super) use context::{TaskControl, TaskInterest};
pub(super) use service::ServiceIdentity;
pub use service::{CpuServiceControl, CpuServiceScope};
pub use step::{CpuTaskDependency, CpuTaskStep};
