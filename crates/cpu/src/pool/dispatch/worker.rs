//! Eligible queue selection and durable condition-variable parking.

use super::Dispatch;

impl Dispatch {
    /// Flexible service prioritizes required background progress, then helps frames.
    pub(super) fn worker(&self, flexible: bool) {
        let mut served_background = false;
        let mut served_retirement = false;
        loop {
            let work = {
                let mut queues = self
                    .queues
                    .lock()
                    .unwrap_or_else(|_| unreachable!("scheduler queue mutations cannot panic"));
                loop {
                    let has_frame = !queues.priority.is_empty()
                        || !queues.urgent.is_empty()
                        || !queues.frame.is_empty();
                    let has_service = !queues.required.is_empty() || !queues.retirement.is_empty();
                    let take_background = flexible
                        && has_service
                        && (self.protected || !served_background || !has_frame);
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
                    } else if flexible {
                        queues.speculative.pop_front()
                    } else {
                        None
                    };
                    if let Some(work) = work {
                        served_background = take_background;
                        self.publish_queued(&queues);
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
            if let Some(work) = work.run(flexible) {
                // The same box, identity and admission lease survive the yield.
                self.resume_service(work);
            }
        }
    }
}
