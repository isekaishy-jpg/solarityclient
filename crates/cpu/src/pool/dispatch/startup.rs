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
    /// Starts exactly the configured workers; the last worker is flexible.
    pub(crate) fn start(
        count: usize,
        capacity: usize,
        budget: &CpuStorageBudget,
    ) -> Result<(Arc<Self>, Vec<JoinHandle<()>>), CpuError> {
        let frame_capacity = count.checked_mul(capacity).ok_or(CpuError::BatchStorage)?;
        let mut frame = StorageDeque::default();
        let mut urgent = StorageDeque::default();
        let mut priority = StorageDeque::default();
        let mut required = StorageDeque::default();
        let mut retirement = StorageDeque::default();
        let mut speculative = StorageDeque::default();
        frame.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Metadata,
            frame_capacity,
        )?;
        urgent.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Metadata,
            frame_capacity,
        )?;
        priority.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Metadata,
            capacity,
        )?;
        // Each bucket can receive all admitted records after a demand change.
        // Queue infrastructure is required metadata even when speculation is off.
        for queue in [&mut required, &mut retirement, &mut speculative] {
            queue.reserve(
                budget,
                CpuStorageClass::Required,
                CpuStorageKind::Metadata,
                capacity,
            )?;
        }
        let shared = Arc::new(Self {
            queues: Mutex::new(Queues {
                frame,
                urgent,
                priority,
                required,
                retirement,
                speculative,
                stopping: false,
            }),
            ready: Condvar::new(),
            queued: AtomicU8::new(0),
            protected: count > 1,
        });
        let environment = WorkerEnvironment::capture();
        let mut handles = Vec::with_capacity(count);
        for index in 0..count {
            let worker = Arc::clone(&shared);
            let flexible = index + 1 == count;
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
                worker.worker(flexible);
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
