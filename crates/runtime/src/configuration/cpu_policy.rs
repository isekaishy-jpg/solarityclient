//! Runtime resolves machine policy; CPU receives only validated execution counts.

use super::ConfigurationError;
use solarity_cpu::CpuExecutionPlan;
use std::{ffi::OsString, num::NonZeroUsize};

/// Preserve the current measured starting split unless explicitly configured.
/// Bulk service consumes flexible capacity rather than adding another pool.
pub(super) fn resolve(
    total: NonZeroUsize,
    flexible: Option<OsString>,
    service: Option<OsString>,
    bulk: Option<OsString>,
) -> Result<CpuExecutionPlan, ConfigurationError> {
    let flexible = count(flexible, "--cpu-flexible-workers")?;
    let service = count(service, "--cpu-service-workers")?;
    let bulk = count(bulk, "--cpu-bulk-limit")?;
    let protected = total
        .get()
        .checked_sub(flexible)
        .ok_or(ConfigurationError::InvalidCpuExecutionPlan)?;
    CpuExecutionPlan::new(protected, flexible, service, bulk)
        .map_err(|_| ConfigurationError::InvalidCpuExecutionPlan)
}

/// Default counts are runtime policy, not a guessed stock compatibility path.
fn count(value: Option<OsString>, option: &'static str) -> Result<usize, ConfigurationError> {
    let Some(value) = value else {
        return Ok(1);
    };
    let text = value
        .to_str()
        .ok_or(ConfigurationError::NonUnicodeValue { option })?;
    text.parse::<NonZeroUsize>()
        .map(NonZeroUsize::get)
        .map_err(|_| ConfigurationError::InvalidPositiveInteger {
            option,
            value: text.to_owned(),
        })
}
