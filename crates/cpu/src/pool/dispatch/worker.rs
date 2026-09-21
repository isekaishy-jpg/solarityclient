//! Eligible queue selection and durable condition-variable parking.

use super::Dispatch;

impl Dispatch {
    /// Reserved service workers guarantee a turn at kernel boundaries. Other
    /// flexible capacity prefers frames; indivisible services share the bulk cap.
    pub(super) fn worker(
        self: &std::sync::Arc<Self>,
        index: usize,
        flexible: bool,
        service_reserved: bool,
        environment: crate::environment::WorkerEnvironment,
    ) {
        let mut worker = super::WorkerLane {
            owner: std::ptr::from_ref(self.as_ref()).addr(),
            index,
            environment,
            execution: crate::CpuServiceExecution::Finite,
        };
        let mut served_background = false;
        let mut served_retirement = false;
        loop {
            let (work, service, bulk, residence, wake) = {
                let mut wake = None;
                let mut queues = self.lock();
                loop {
                    let has_frame = !queues.priority.is_empty()
                        || !queues.urgent.is_empty()
                        || !queues.frame.is_empty();
                    let bulk_available = queues.active_bulk < self.bulk_limit;
                    let has_required = queues.required.eligible(bulk_available);
                    let has_retirement = queues.retirement.eligible(bulk_available);
                    let has_service = has_required || has_retirement;
                    let take_background = flexible
                        && has_service
                        && (!has_frame
                            || (service_reserved && (self.protected || !served_background)));
                    let work = if take_background {
                        // One finite turn per class prevents either backlog from
                        // monopolizing service. A single indivisible call can still be slow.
                        let retire = has_retirement && (!served_retirement || !has_required);
                        served_retirement = retire;
                        if retire {
                            queues.retirement.pop_eligible(bulk_available)
                        } else {
                            queues.required.pop_eligible(bulk_available)
                        }
                    } else if has_frame {
                        queues
                            .priority
                            .pop_front()
                            .or_else(|| queues.urgent.pop())
                            .or_else(|| queues.frame.pop())
                    } else if flexible {
                        queues.speculative.pop_eligible(bulk_available)
                    } else {
                        None
                    };
                    if let Some(work) = work {
                        served_background = take_background;
                        let service = work.service_identity().is_some();
                        let bulk = service && work.execution() == crate::CpuServiceExecution::Bulk;
                        queues.active_bulk += usize::from(bulk);
                        self.publish_queued(&queues);
                        let (work, residence) = work.take();
                        break (work, service, bulk, residence, wake);
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
            worker.execution = work.execution();
            let resumed = work.run(service_reserved, worker);
            // A withdrawn result can be destroyed during publication, after the
            // adapter's pre-publication restore. That destructor also belongs to
            // the foreign service boundary, before this lane runs another kernel.
            if service {
                assert!(
                    worker.environment.install(),
                    "initialized worker numeric controls remain supported"
                );
            }
            if service {
                let mut queues = self.lock();
                queues.active_bulk -= usize::from(bulk);
                let binding = match resumed {
                    Some(super::Continuation::Ready(work)) => {
                        let queued = crate::pool::observation::SampleTime::now();
                        queues
                            .service(work.service())
                            .push_back(super::QueuedWork::new(work, queued));
                        None
                    }
                    Some(super::Continuation::Waiting(work, dependency)) => {
                        queues.park(work, dependency)
                    }
                    None => None,
                };
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
                // Readiness publication may already have won the race. Binding
                // replays that durable result outside the dispatch lock.
                if let Some((binding, serial, index)) = binding {
                    self.follow_parked_demand(serial, index);
                    let sink: std::sync::Arc<dyn crate::completion::ReadySink> = self.clone();
                    binding.bind(std::sync::Arc::downgrade(&sink), serial, index);
                }
            } else {
                debug_assert!(resumed.is_none());
            }
        }
    }
}
