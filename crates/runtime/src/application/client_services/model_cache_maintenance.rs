//! Cache deadlines admit finite cleanup without polling every model or blocking main.

use solarity_asset::M2CacheService;
use solarity_cpu::{CpuError, CpuExecutor, CpuService, CpuTask};
use std::time::{Duration, Instant};

/// Runtime owns the deadline and task; assets own qualification and release clocks.
pub(super) struct RuntimeModelCacheMaintenance {
    sources: M2CacheService,
    pending: Option<CpuTask<()>>,
    deadline: Option<Instant>,
}

impl RuntimeModelCacheMaintenance {
    /// Catalog clones and their mounted stores publish into this one namespace service.
    pub(super) fn new(sources: M2CacheService) -> Self {
        Self {
            sources,
            pending: None,
            deadline: None,
        }
    }

    /// Ordinary frames check only a change bit, a task flag and the cached deadline.
    /// CPU capacity is reserved before moving any collection or source ownership.
    pub(super) fn service(&mut self, cpu: &CpuExecutor) -> Result<(), CpuError> {
        self.service_at(cpu, Instant::now())
    }

    /// Orderly shutdown observes any accepted cleanup result before the pool drains.
    pub(super) fn finish(&mut self) -> Result<(), CpuError> {
        if let Some(task) = self.pending.take() {
            task.join()?;
        }
        Ok(())
    }

    /// Explicit coordinator time keeps admission tests independent of wall-clock pacing.
    fn service_at(&mut self, cpu: &CpuExecutor, now: Instant) -> Result<(), CpuError> {
        let mut completed = false;
        if self.pending.as_ref().is_some_and(CpuTask::is_finished) {
            let task = self
                .pending
                .take()
                .unwrap_or_else(|| unreachable!("observed completion remains owned"));
            task.join()?;
            completed = true;
        }
        if self.pending.is_some() {
            return Ok(());
        }
        if self.sources.take_changed()
            || completed
            || self.deadline.is_some_and(|deadline| now >= deadline)
        {
            self.deadline = self
                .sources
                .next_collection_delay_ms()
                .map(|delay| now + Duration::from_millis(u64::from(delay)));
        }
        if self.deadline.is_none_or(|deadline| now < deadline) {
            return Ok(());
        }
        let permit = match cpu.try_reserve_for(CpuService::Retirement) {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(()),
            Err(error) => return Err(error),
        };
        let sources = self.sources.clone();
        let mut collection = None;
        self.pending = Some(permit.submit_steps(move || {
            let _profile = solarity_profiling::profile!("assets.m2_cache.retire");
            let collection = collection.get_or_insert_with(|| sources.begin_collection());
            collection.step()
        }));
        self.deadline = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/application/model_cache_maintenance.rs"]
mod tests;
