//! Persistent protected/flexible workers and durable ready-queue predicates.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use super::CpuError;
use crate::environment::WorkerEnvironment;

/// Reusable typed work is erased only at the queue boundary.
pub(crate) trait ReadyWork: Send + Sync {
    /// Executes an admitted portion without waiting for another worker job.
    fn run(&self);
}

/// Eligibility keeps blocking service off the protected frame workers.
#[derive(Clone, Copy)]
pub(crate) enum WorkClass {
    Frame,
    Background,
}

/// Cold background closures and retained frame operations share thread ownership.
pub(crate) enum Work {
    Once(Box<dyn FnOnce() + Send>),
    Retained(Arc<dyn ReadyWork>),
}

impl Work {
    /// Runs outside every scheduler lock.
    fn run(self) {
        match self {
            Self::Once(operation) => operation(),
            Self::Retained(operation) => operation.run(),
        }
    }
}

/// Both queue predicates and shutdown are changed under the same mutex.
struct Queues {
    frame: VecDeque<Work>,
    background: VecDeque<Work>,
    stopping: bool,
}

/// The pool owns all handles; no task creates or detaches a thread.
pub(crate) struct Dispatch {
    queues: Mutex<Queues>,
    ready: Condvar,
}

impl Dispatch {
    /// Starts exactly the configured workers; the last worker is flexible.
    pub(crate) fn start(
        count: usize,
        capacity: usize,
    ) -> Result<(Arc<Self>, Vec<JoinHandle<()>>), CpuError> {
        let shared = Arc::new(Self {
            queues: Mutex::new(Queues {
                frame: VecDeque::with_capacity(count),
                background: VecDeque::with_capacity(capacity),
                stopping: false,
            }),
            ready: Condvar::new(),
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

    /// Enqueues already admitted work before waking eligible sleepers.
    pub(crate) fn push(&self, work: Work, class: WorkClass) {
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"));
        match class {
            WorkClass::Frame => queues.frame.push_back(work),
            WorkClass::Background => queues.background.push_back(work),
        }
        drop(queues);
        // A single notify could wake a protected worker for background work.
        self.ready.notify_all();
    }

    /// Closes the durable sleep predicate after the admission owners drain.
    pub(crate) fn stop(&self) {
        self.queues
            .lock()
            .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"))
            .stopping = true;
        self.ready.notify_all();
    }

    /// Flexible service prioritizes required background progress, then helps frames.
    fn worker(&self, flexible: bool) {
        loop {
            let work = {
                let mut queues = self
                    .queues
                    .lock()
                    .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"));
                loop {
                    let work = if flexible {
                        queues
                            .background
                            .pop_front()
                            .or_else(|| queues.frame.pop_front())
                    } else {
                        queues.frame.pop_front()
                    };
                    if let Some(work) = work {
                        break work;
                    }
                    if queues.stopping {
                        return;
                    }
                    queues = self
                        .ready
                        .wait(queues)
                        .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"));
                }
            };
            work.run();
        }
    }
}
