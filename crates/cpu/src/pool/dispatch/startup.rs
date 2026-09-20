//! Persistent worker construction and numeric-environment handshakes.

use super::{CpuError, Dispatch, Queues};
use crate::environment::WorkerEnvironment;
use crate::storage::StorageDeque;
use crate::{CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::{
    sync::{Arc, Condvar, Mutex, atomic::AtomicU8},
    thread::{self, JoinHandle},
};

impl Dispatch {
    /// Starts the resolved protected/flexible split within one thread allowance.
    pub(crate) fn start(
        plan: crate::CpuExecutionPlan,
        capacity: usize,
        budget: &CpuStorageBudget,
    ) -> Result<(Arc<Self>, Vec<JoinHandle<()>>), CpuError> {
        let count = plan.worker_count().get();
        let frame_capacity = count.checked_mul(capacity).ok_or(CpuError::BatchStorage)?;
        let priority_capacity = capacity.checked_mul(2).ok_or(CpuError::BatchStorage)?;
        let service_capacity = plan
            .flexible_workers()
            .get()
            .checked_mul(capacity)
            .ok_or(CpuError::BatchStorage)?;
        let mut frame = super::cost::CostQueue::default();
        let mut urgent = super::cost::CostQueue::default();
        let mut priority = StorageDeque::default();
        let mut required = super::service::ServiceQueue::default();
        let mut retirement = super::service::ServiceQueue::default();
        let mut speculative = super::service::ServiceQueue::default();
        let mut sleepers = crate::storage::StorageVec::default();
        sleepers.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Metadata,
            count,
        )?;
        sleepers.resize_with(count, super::SleepingWorker::default);
        frame.reserve(budget, frame_capacity)?;
        urgent.reserve(budget, frame_capacity)?;
        priority.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Metadata,
            priority_capacity,
        )?;
        // Each bucket can receive every admitted graph's flexible runners after
        // a demand change. Single-call services consume only one such record.
        // Queue infrastructure is required metadata even when speculation is off.
        for queue in [&mut required, &mut retirement, &mut speculative] {
            queue.reserve(budget, service_capacity)?;
        }
        let shared = Arc::new(Self {
            queues: Mutex::new(Queues {
                frame,
                urgent,
                priority,
                required,
                retirement,
                speculative,
                sleepers,
                stopping: false,
                active_bulk: 0,
            }),
            ready: Condvar::new(),
            queued: AtomicU8::new(0),
            protected: plan.protected_workers() != 0,
            bulk_limit: plan.bulk_limit().get(),
            flexible_workers: plan.flexible_workers().get(),
        });
        let environment = WorkerEnvironment::capture();
        let mut handles = Vec::with_capacity(count);
        for index in 0..count {
            let worker = Arc::clone(&shared);
            let flexible = index >= plan.protected_workers();
            let service_reserved =
                flexible && index - plan.protected_workers() < plan.service_reserve().get();
            let name = if flexible {
                format!("solarity-flex-{index}")
            } else {
                format!("solarity-frame-{index}")
            };
            let (initialized, observed) = std::sync::mpsc::sync_channel(1);
            match thread::Builder::new().name(name).spawn(move || {
                let ready = environment.install();
                if initialized.send(ready).is_err() || !ready {
                    return;
                }
                worker.worker(index, flexible, service_reserved, environment);
            }) {
                Ok(handle) => {
                    handles.push(handle);
                    if observed.recv() != Ok(true) {
                        shared.stop();
                        for handle in handles {
                            let _joined = handle.join();
                        }
                        return Err(CpuError::PoolBuild {
                            message: format!(
                                "worker {index} numeric environment initialization failed"
                            ),
                        });
                    }
                }
                Err(error) => {
                    shared.stop();
                    for handle in handles {
                        let _joined = handle.join();
                    }
                    return Err(CpuError::PoolBuild {
                        message: error.to_string(),
                    });
                }
            }
        }
        Ok((shared, handles))
    }
}
