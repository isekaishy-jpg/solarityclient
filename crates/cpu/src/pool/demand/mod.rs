//! Registered consumer demand controls one producer independently of result ownership.

mod interest;
mod owner;

use super::{CpuService, CpuServiceControl};
use std::sync::{Arc, Mutex, atomic::AtomicU8};

/// Shared demand contains scheduler metadata only, never domain inputs or callbacks.
#[derive(Default)]
struct State {
    values: Mutex<Values>,
}

/// Lock order is demand metadata then dispatch queues; dispatch never acquires demand state.
#[derive(Default)]
struct Values {
    counts: [usize; 3],
    control: Option<CpuServiceControl>,
}

/// Last clone releases one logical consumer, regardless of handle-copy count.
struct Interest {
    state: Arc<State>,
    service: AtomicU8,
}

/// Independent consumers combine their strongest live service requirement.
#[derive(Clone, Default)]
pub struct CpuServiceDemand(Arc<State>);

/// One registered consumer can change or withdraw demand without cancelling other consumers.
#[derive(Clone)]
pub struct CpuServiceInterest(Arc<Interest>);

impl Values {
    /// Empty demand drains as retirement; speculative ownership cannot demote a required consumer.
    fn publish(&self) {
        let service = if self.counts[CpuService::Required as usize] != 0 {
            CpuService::Required
        } else if self.counts[CpuService::Retirement as usize] != 0
            || self.counts[CpuService::Speculative as usize] == 0
        {
            CpuService::Retirement
        } else {
            CpuService::Speculative
        };
        if let Some(control) = &self.control {
            control.set_service(service);
        }
    }
}
