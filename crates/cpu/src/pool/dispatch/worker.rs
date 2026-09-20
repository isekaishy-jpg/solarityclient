//! Eligible queue selection and durable condition-variable parking.

use super::Dispatch;

impl Dispatch {
    /// Reserved service workers guarantee a turn at kernel boundaries. Other
    /// flexible capacity prefers frames; every service call shares the bulk cap.
    pub(super) fn worker(&self, index: usize, flexible: bool, service_reserved: bool) {
        let mut served_background = false;
        let mut served_retirement = false;
        loop {
            let (work, service, residence, wake) = {
                let mut wake = None;
                let mut queues = self.lock();
                loop {
                    let has_frame = !queues.priority.is_empty()
                        || !queues.urgent.is_empty()
                        || !queues.frame.is_empty();
                    let has_service = !queues.required.is_empty() || !queues.retirement.is_empty();
                    let service_available = flexible && queues.active_service < self.bulk_limit;
                    let take_background = flexible
                        && service_available
                        && has_service
                        && (!has_frame
                            || (service_reserved && (self.protected || !served_background)));
                    let work = if take_background {
                        // One finite turn per class prevents either backlog from
                        // monopolizing service. A single indivisible call can still be slow.
                        let retire = !queues.retirement.is_empty()
                            && (!served_retirement || queues.required.is_empty());
                        served_retirement = retire;
                        if retire {
                            queues.retirement.pop_front()
                        } else {
                            queues.required.pop_front()
                        }
                    } else if has_frame {
                        queues
                            .priority
                            .pop_front()
                            .or_else(|| queues.urgent.pop())
                            .or_else(|| queues.frame.pop())
                    } else if service_available {
                        queues.speculative.pop_front()
                    } else {
                        None
                    };
                    if let Some(work) = work {
                        served_background = take_background;
                        let service = work.service_identity().is_some();
                        queues.active_service += usize::from(service);
                        self.publish_queued(&queues);
                        let (work, residence) = work.take();
                        break (work, service, residence, wake);
                    }
                    if queues.stopping {
                        return;
                    }
                    queues.sleepers[index].park();
                    queues = queues.wait(&self.ready);
                    wake = queues.sleepers[index].returned(flexible);
                }
            };
            if let Some(residence) = residence {
                residence.report();
            }
            if let Some(wake) = wake {
                wake.report();
            }
            // Only workers with reserved service turns yield a frame runner for
            // background demand. Other flexible workers keep assisting frames.
            let resumed = work.run(service_reserved);
            if service {
                let mut queues = self.lock();
                queues.active_service -= 1;
                if let Some(work) = resumed {
                    let queued = crate::pool::observation::SampleTime::now();
                    queues
                        .service(work.service())
                        .push_back(super::QueuedWork::new(work, queued));
                }
                self.publish_queued(&queues);
                let wake_service = self.flexible_workers > 1
                    && (!queues.required.is_empty()
                        || !queues.retirement.is_empty()
                        || !queues.speculative.is_empty());
                // A completed call opens shared bulk eligibility for sleepers.
                // A sole flexible worker observes its own returned work without
                // waking protected sleepers for every retirement step.
                if wake_service {
                    self.notify_ready(queues);
                } else {
                    drop(queues);
                }
            } else {
                debug_assert!(resumed.is_none());
            }
        }
    }
}
