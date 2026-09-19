//! Controlled worker occupation proves that main preparation returns before pose or geometry consumption.

use solarity_cpu::{CpuExecutor, FrameBatch};
use std::error::Error;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

/// One finite test kernel occupies a worker until the main continuation releases it.
struct HeldJob {
    gate: Arc<(Mutex<bool>, Condvar)>,
    entered: mpsc::Sender<()>,
}

impl HeldJob {
    fn run(&mut self) {
        self.entered
            .send(())
            .unwrap_or_else(|_| unreachable!("test observes worker entry"));
        let (lock, ready) = &*self.gate;
        let mut released = lock
            .lock()
            .unwrap_or_else(|_| unreachable!("test gate is unpoisoned"));
        while !*released {
            released = ready
                .wait(released)
                .unwrap_or_else(|_| unreachable!("test gate is unpoisoned"));
        }
    }
}

/// Every worker is occupied before admission. The timeout only turns a forbidden
/// main-thread join into a test failure instead of hanging the workspace suite.
pub(crate) struct HeldFrameWorkers {
    pending: FrameBatch<HeldJob>,
    jobs: Vec<HeldJob>,
    release: Option<mpsc::Sender<()>>,
    watchdog: Option<JoinHandle<bool>>,
}

impl HeldFrameWorkers {
    pub(crate) fn new(cpu: &CpuExecutor) -> Result<Self, Box<dyn Error>> {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let (release, released) = mpsc::channel();
        let watched = Arc::clone(&gate);
        let watchdog = std::thread::spawn(move || {
            let progressed = released.recv_timeout(Duration::from_secs(30)).is_ok();
            let (lock, ready) = &*watched;
            *lock
                .lock()
                .unwrap_or_else(|_| unreachable!("test gate is unpoisoned")) = true;
            ready.notify_all();
            progressed
        });
        let (entered, arrivals) = mpsc::channel();
        let mut held = Self {
            pending: FrameBatch::new(HeldJob::run),
            jobs: (0..cpu.worker_count())
                .map(|_| HeldJob {
                    gate: Arc::clone(&gate),
                    entered: entered.clone(),
                })
                .collect(),
            release: Some(release),
            watchdog: Some(watchdog),
        };
        held.pending.start(cpu, &mut held.jobs)?;
        for _ in 0..cpu.worker_count() {
            arrivals.recv_timeout(Duration::from_secs(10))?;
        }
        Ok(held)
    }

    /// Main calls this only after admission returns and its independent work can run.
    pub(crate) fn release(mut self) -> Result<(), Box<dyn Error>> {
        self.finish()
    }

    /// Always opens the gate before joining, including when fixture setup fails.
    fn finish(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(release) = self.release.take() {
            // A watchdog timeout has already opened the gate; join reports it below.
            let _ = release.send(());
        }
        let progressed = self
            .watchdog
            .take()
            .map(|watchdog| watchdog.join().map_err(|_| "test watchdog panicked"))
            .transpose()?;
        self.pending.reclaim(&mut self.jobs)?;
        if progressed == Some(false) {
            return Err(
                "M2 admission waited for occupied workers before main could continue".into(),
            );
        }
        Ok(())
    }
}

impl Drop for HeldFrameWorkers {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
