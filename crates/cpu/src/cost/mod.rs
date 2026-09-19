//! Value-only calibration and scheduling hints; estimates never affect validity.

mod calibration;
mod estimate;

pub use calibration::{CostCalibration, WorkMeasurement};
pub use estimate::JobCost;
