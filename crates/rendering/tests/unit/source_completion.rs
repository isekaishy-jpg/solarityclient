//! Native task waits retain typed results and reclaim suspended source owners.

use super::{WorldFrameExecution, WorldRecordingCompletion};
use crate::VulkanError;
use solarity_cpu::{CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize};

fn pool() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

struct Execution<'a> {
    cpu: &'a CpuExecutor,
    failure: u8,
}
impl WorldFrameExecution for Execution<'_> {
    fn executor(&self) -> &CpuExecutor {
        self.cpu
    }
    fn wait_for_gpu(&mut self, completion: &crate::GpuCompletion<'_>) -> Result<(), VulkanError> {
        completion.wait()
    }
    fn wait_for_recording(
        &mut self,
        completion: &WorldRecordingCompletion<'_>,
    ) -> Result<(), VulkanError> {
        match self.failure {
            1 => Err(VulkanError::WorldFrameCapacity),
            2 => panic!("intentional native servicing unwind"),
            _ => {
                completion.wait()?;
                assert!(completion.is_ready());
                completion.wait()
            }
        }
    }
}

#[test]
fn source_wait_keeps_success_and_worker_failure_for_the_typed_consumer()
-> Result<(), Box<dyn Error>> {
    let cpu = pool()?;
    let mut execution = Execution {
        cpu: &cpu,
        failure: 0,
    };
    let task = cpu.try_submit(|| vec![11, 23])?;
    assert_eq!(
        WorldRecordingCompletion::join_task(&mut execution, task)?,
        vec![11, 23]
    );
    let task = cpu.try_submit(|| -> usize { panic!("intentional source panic") })?;
    assert!(matches!(
        WorldRecordingCompletion::join_task(&mut execution, task),
        Err(VulkanError::Cpu(CpuError::TaskPanicked))
    ));
    Ok(())
}

#[test]
fn source_wait_failure_and_unwind_cancel_and_join_a_suspended_consumer()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuStorageClass, CpuTaskDependency, CpuTaskStep, SharedProduct};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;
    let cpu = pool()?;
    for failure in [1, 2] {
        let (_producer, product) =
            SharedProduct::<(), ()>::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let mut edge = Some(CpuTaskDependency::new(&product.readiness())?);
        let retired = Arc::new(AtomicBool::new(false));
        let worker_retired = Arc::clone(&retired);
        let (notice, observed) = mpsc::channel();
        let task = cpu
            .try_reserve()?
            .submit_resumable_with_context(move |context| {
                if context.is_cancelled() {
                    worker_retired.store(true, Ordering::Release);
                    return CpuTaskStep::Complete(());
                }
                let _ = notice.send(());
                CpuTaskStep::Wait(
                    edge.take()
                        .unwrap_or_else(|| unreachable!("one suspended turn")),
                )
            });
        observed.recv_timeout(Duration::from_secs(5))?;
        let mut execution = Execution { cpu: &cpu, failure };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            WorldRecordingCompletion::join_task(&mut execution, task)
        }));
        if failure == 1 {
            assert!(matches!(result, Ok(Err(VulkanError::WorldFrameCapacity))));
        } else {
            assert!(result.is_err());
        }
        assert!(
            retired.load(Ordering::Acquire),
            "native failure reclaims before returning"
        );
        assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    }
    Ok(())
}
