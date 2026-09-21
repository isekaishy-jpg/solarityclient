//! One resumable appearance owns its bank through source waits and withdrawal.

use super::super::AppearanceCompletion;
use solarity_asset::M2LoadRequest;
use solarity_cpu::{CpuError, CpuService, CpuTask};

pub(in crate::application::player_coordinator) enum AppearanceTask<T: Send + 'static> {
    Direct {
        task: CpuTask<AppearanceCompletion<T>>,
        demand: Option<M2LoadRequest>,
    },
}

impl<T: Send + 'static> AppearanceTask<T> {
    pub(in crate::application::player_coordinator) fn is_finished(&self) -> bool {
        let Self::Direct { task, .. } = self;
        task.is_finished()
    }

    /// Withdrawal wakes a suspended consumer. An already claimed source producer
    /// still completes for its other subscribers before returning the bank.
    pub(in crate::application::player_coordinator) fn retire(&mut self) {
        let Self::Direct { task, demand } = self;
        if let Some(demand) = demand {
            demand.set_service(CpuService::Retirement);
        } else {
            task.set_service(CpuService::Retirement);
        }
        task.cancel();
    }

    pub(in crate::application::player_coordinator) fn join(
        self,
    ) -> Result<AppearanceCompletion<T>, CpuError> {
        let Self::Direct { task, .. } = self;
        task.join()
    }
}
