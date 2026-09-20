//! Queue residence and useful native wake intervals retain fixed-size samples.

use super::{ReadyWork, Work};
use crate::pool::observation::{Measurement, SampleTime};
use solarity_profiling::Site;
use std::{ops::Deref, sync::Arc};

/// Queue metadata is preadmitted with the queue, including optional observation.
pub(super) struct QueuedWork {
    work: Work,
    queued: Option<SampleTime>,
}

impl QueuedWork {
    /// Batch publication shares one timestamp without adding a clock per runner.
    pub fn new(work: Work, queued: Option<SampleTime>) -> Self {
        Self { work, queued }
    }

    /// End residence while still holding the queue guard. Emit after releasing it.
    pub fn take(self) -> (Work, Option<Measurement>) {
        static FRAME: Site = Site::new("cpu.dispatch.frame.queue_wait", true);
        static SERVICE: Site = Site::new("cpu.dispatch.service.queue_wait", true);
        static PRIORITY: Site = Site::new("cpu.dispatch.priority.queue_wait", true);
        let site = match &self.work {
            Work::Retained(_) => &FRAME,
            Work::Priority(..) => &PRIORITY,
            _ => &SERVICE,
        };
        (self.work, self.queued.map(|time| time.finish(site)))
    }

    /// Reclassification moves the original record without resetting queue age.
    pub fn belongs_to(&self, owner: &Arc<dyn ReadyWork>) -> bool {
        self.work.belongs_to(owner)
    }
}

impl Deref for QueuedWork {
    type Target = Work;

    fn deref(&self) -> &Self::Target {
        &self.work
    }
}

/// First notification after parking is retained; repeated publication cannot
/// shorten an outstanding wake. Idle time before publication is never included.
#[derive(Default)]
pub(super) struct SleepingWorker {
    parked: bool,
    notified: Option<SampleTime>,
}

impl SleepingWorker {
    /// The caller still owns the queue lock until its atomic condition wait.
    pub fn park(&mut self) {
        self.parked = true;
        self.notified = None;
    }

    /// Preserve the earliest notification while this worker remains parked.
    pub fn notify(&mut self, time: SampleTime) {
        if self.parked && self.notified.is_none() {
            self.notified = Some(time);
        }
    }

    /// The endpoint includes native scheduling and mutex reacquisition. The
    /// worker reports it only when this wake discovers eligible work.
    pub fn returned(&mut self, flexible: bool) -> Option<Measurement> {
        static PROTECTED: Site = Site::new("cpu.dispatch.protected.wake", true);
        static FLEXIBLE: Site = Site::new("cpu.dispatch.flexible.wake", true);
        self.parked = false;
        self.notified
            .take()
            .map(|start| start.finish(if flexible { &FLEXIBLE } else { &PROTECTED }))
    }
}
