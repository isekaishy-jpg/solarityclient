//! Transitive external failure progresses on one worker without recursive kernels.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, FrameBatch, FrameBatchPlan, JobOutcome,
};
use std::{error::Error, num::NonZeroUsize};

#[test]
fn a_failed_external_phase_propagates_through_chained_ready_gates() -> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(64).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let port = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let mut producer = port.producer()?;
    let mut token = port.readiness();
    let mut batches = Vec::new();
    for _ in 0..32 {
        let mut batch = FrameBatch::new(|value| *value += 1u32);
        batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token)?;
        batch.push(&mut Some(0))?;
        batch.close();
        token = batch.completion()?;
        batches.push(batch);
    }
    producer.complete(JobOutcome::Failed)?;
    let tail = batches.last_mut().ok_or("tail")?;
    assert!(matches!(
        tail.with_result(&tail.job(0)?, |value| *value),
        Err(CpuError::DependencyFailed)
    ));
    let mut returned = Vec::new();
    for batch in &mut batches {
        assert!(matches!(
            batch.reclaim(&mut returned),
            Err(CpuError::DependencyFailed)
        ));
    }
    assert_eq!(returned, vec![0; 32]);
    Ok(())
}
