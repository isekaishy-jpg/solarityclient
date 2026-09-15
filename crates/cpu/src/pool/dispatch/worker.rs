//! Eligible queue selection and durable condition-variable parking.

use super::Dispatch;

impl Dispatch {
    /// Flexible service prioritizes required background progress, then helps frames.
    pub(super) fn worker(&self, flexible: bool) {
        let mut served_background = false;
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
                    let take_background = flexible
                        && !queues.background.is_empty()
                        && (!served_background || !has_frame);
                    let work = if take_background {
                        queues.background.pop_front()
                    } else {
                        queues
                            .priority
                            .pop_front()
                            .or_else(|| queues.urgent.pop_front())
                            .or_else(|| queues.frame.pop_front())
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
            work.run(flexible);
        }
    }
}
