//! World continuation ordering with a real independently completed receiver phase.

use super::{MainPreparationStep, WorldMainPreparation};
use crate::application::frame_pipeline::FrameWait;
use solarity_cpu::{
    CompletionPort, CpuExecutor, CpuPoolConfig, CpuStorageClass, CpuStoragePlan, JobOutcome,
};
use std::{error::Error, num::NonZeroUsize};

fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?)
}

#[test]
fn surfaces_run_before_receivers_complete_and_uniforms_wake_main() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut main = WorldMainPreparation::new(&cpu)?;
    let mut frame = main.begin(&cpu)?;
    assert_eq!(
        frame.take_ready(),
        Some((MainPreparationStep::GroundDetail, JobOutcome::Succeeded))
    );
    assert!(frame.take_ready().is_none());
    frame.ground_finished(true)?;
    assert_eq!(
        frame.take_ready(),
        Some((MainPreparationStep::WorldModels, JobOutcome::Succeeded))
    );
    assert!(frame.initial_done());
    let receivers = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut producer = receivers.producer()?;
    frame.publish_sources(Some(receivers.readiness()))?;
    assert_eq!(
        frame.take_ready(),
        Some((MainPreparationStep::Surfaces, JobOutcome::Succeeded))
    );
    assert!(frame.take_ready().is_none());
    assert!(frame.needs_uniform_notice());
    let worker = cpu.try_submit(move || producer.complete(JobOutcome::Succeeded))?;
    frame.wait(&mut FrameWait::Offline)?;
    assert_eq!(
        frame.take_ready(),
        Some((MainPreparationStep::Uniforms, JobOutcome::Succeeded))
    );
    assert!(!frame.needs_uniform_notice());
    worker.join()??;
    Ok(())
}

#[test]
fn failed_ground_skips_dependents_without_preventing_model_cleanup() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut main = WorldMainPreparation::new(&cpu)?;
    let mut frame = main.begin(&cpu)?;
    frame.take_ready().ok_or("ground")?;
    frame.ground_finished(false)?;
    assert_eq!(
        frame.take_ready(),
        Some((
            MainPreparationStep::WorldModels,
            JobOutcome::DependencyFailed
        ))
    );
    assert!(frame.initial_done());
    assert_eq!(
        frame.take_ready(),
        Some((MainPreparationStep::Surfaces, JobOutcome::DependencyFailed))
    );
    frame.publish_sources(None)?;
    assert_eq!(
        frame.take_ready(),
        Some((MainPreparationStep::Uniforms, JobOutcome::Succeeded))
    );
    Ok(())
}

#[test]
fn abandoned_world_epoch_cannot_publish_into_the_next_frame() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut main = WorldMainPreparation::new(&cpu)?;
    let receivers = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut producer = receivers.producer()?;
    {
        let mut frame = main.begin(&cpu)?;
        frame.take_ready().ok_or("ground")?;
        frame.ground_finished(true)?;
        frame.take_ready().ok_or("world models")?;
        frame.publish_sources(Some(receivers.readiness()))?;
    }
    let mut next = main.begin(&cpu)?;
    producer.complete(JobOutcome::Succeeded)?;
    assert_eq!(
        next.take_ready(),
        Some((MainPreparationStep::GroundDetail, JobOutcome::Succeeded))
    );
    assert!(next.take_ready().is_none());
    Ok(())
}
