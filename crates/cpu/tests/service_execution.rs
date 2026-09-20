//! Service eligibility and foreign-state isolation under the real worker pool.

use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuServiceExecution, CpuStoragePlan,
    FrameBatch, JobOutcome, LoadBatch,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc, time::Duration};

/// All configured threads belong to this pool; no hidden service executor exists.
fn executor(flexible: usize, bulk: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, flexible, 1, bulk)?,
        NonZeroUsize::new(16).ok_or("capacity")?,
        CpuStoragePlan::new(8 << 20, 8 << 20, 8 << 20),
    ))?)
}

/// A blocked bulk call cannot consume another flexible worker's finite eligibility.
#[test]
fn finite_service_progresses_while_the_bulk_allowance_is_full() -> Result<(), Box<dyn Error>> {
    let cpu = executor(2, 1)?;
    let (entered, observed) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let blocked = cpu.try_submit(move || {
        let _sent = entered.send(());
        let _released = wait.recv_timeout(Duration::from_secs(10));
    })?;
    observed.recv_timeout(Duration::from_secs(5))?;
    let (completed, result) = mpsc::channel();
    let finite = cpu
        .try_reserve()?
        .with_execution(CpuServiceExecution::Finite)
        .submit_with_context(move |context| completed.send(context.execution()));
    let received = result.recv_timeout(Duration::from_secs(5));
    let _released = release.send(());
    blocked.join()?;
    finite.join()??;
    assert_eq!(received?, CpuServiceExecution::Finite);
    Ok(())
}

/// Each queued graph runner retains its ownership across repeated demand changes.
#[test]
fn promotion_preserves_every_loading_runner() -> Result<(), Box<dyn Error>> {
    let cpu = executor(3, 3)?;
    let (entered, observed) = mpsc::channel();
    let mut blockers = Vec::new();
    let mut releases = Vec::new();
    for _ in 0..3 {
        let (release, wait) = mpsc::channel();
        let entered = entered.clone();
        blockers.push(cpu.try_submit(move || {
            let _sent = entered.send(());
            let _released = wait.recv_timeout(Duration::from_secs(10));
        })?);
        releases.push(release);
    }
    for _ in 0..3 {
        observed.recv_timeout(Duration::from_secs(5))?;
    }
    let mut load =
        LoadBatch::with_context(CpuService::Speculative, |value: &mut usize, context| {
            assert_eq!(context.execution(), CpuServiceExecution::Bulk);
            *value += 1;
            JobOutcome::Succeeded
        });
    let mut values: Vec<_> = (0..64).collect();
    load.start_after(&cpu, &mut values, &[])?;
    let control = load.service_control()?;
    for service in [
        CpuService::Required,
        CpuService::Retirement,
        CpuService::Speculative,
        CpuService::Required,
    ] {
        control.set_service(service);
    }
    for release in releases {
        let _released = release.send(());
    }
    for blocker in blockers {
        blocker.join()?;
    }
    load.reclaim(&mut values)?;
    assert_eq!(values, (1..=64).collect::<Vec<_>>());
    Ok(())
}

/// A graph's independent resource nodes occupy distinct flexible lanes before
/// any node is released. Count alone cannot pass this handshake with one runner.
#[test]
fn independent_loading_nodes_use_multiple_flexible_workers() -> Result<(), Box<dyn Error>> {
    let cpu = executor(3, 3)?;
    let (entered, observed) = mpsc::channel();
    let mut releases = Vec::new();
    let mut inputs = Vec::new();
    for _ in 0..3 {
        let (release, wait) = mpsc::channel();
        inputs.push((entered.clone(), wait));
        releases.push(release);
    }
    let mut load = LoadBatch::with_context(
        CpuService::Required,
        |input: &mut (mpsc::Sender<()>, mpsc::Receiver<()>), _| {
            let _entered = input.0.send(());
            let _release = input.1.recv_timeout(Duration::from_secs(10));
            JobOutcome::Succeeded
        },
    );
    load.start_after(&cpu, &mut inputs, &[])?;
    let ready = (0..3).try_for_each(|_| observed.recv_timeout(Duration::from_secs(5)));
    for release in releases {
        let _released = release.send(());
    }
    load.reclaim(&mut inputs)?;
    ready?;
    assert_eq!(inputs.len(), 3);
    Ok(())
}

/// Observe control bits only; floating-point status flags are deliberately excluded.
#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
fn controls() -> u32 {
    let mut value = 0_u32;
    // SAFETY: x86-64 guarantees SSE and the destination is a writable u32.
    unsafe {
        std::arch::asm!("stmxcsr [{address}]", address = in(reg) &mut value, options(nostack, preserves_flags));
    }
    value & !0x3f
}

/// Simulates a foreign decoder changing the worker's rounding control.
#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
fn change_controls() {
    let value = controls() ^ 0x2000;
    // SAFETY: retain all supported control bits and change only valid rounding mode.
    unsafe {
        std::arch::asm!("ldmxcsr [{address}]", address = in(reg) &value, options(nostack, preserves_flags));
    }
}

/// Both a successful decoder and its unwind restore the next parity kernel's controls.
#[cfg(target_arch = "x86_64")]
#[test]
fn foreign_service_restores_numeric_controls_before_frame_kernels() -> Result<(), Box<dyn Error>> {
    let expected = controls();
    let cpu = executor(1, 1)?;
    for unwind in [false, true] {
        let task = cpu.try_submit(move || {
            change_controls();
            assert!(!unwind, "controlled foreign failure");
        })?;
        assert_eq!(task.join().is_err(), unwind);
        let mut frame = FrameBatch::new(|value: &mut u32| *value = controls());
        let mut values = vec![0];
        frame.start(&cpu, &mut values)?;
        frame.reclaim(&mut values)?;
        assert_eq!(values, [expected]);
    }
    Ok(())
}

/// An abandoned foreign result is destroyed on its worker, including numeric
/// changes or panic in its destructor. Neither can contaminate the next frame.
#[cfg(target_arch = "x86_64")]
#[test]
fn withdrawn_result_cleanup_preserves_the_worker_lane() -> Result<(), Box<dyn Error>> {
    struct ForeignResult(bool);
    impl Drop for ForeignResult {
        fn drop(&mut self) {
            change_controls();
            assert!(!self.0, "controlled foreign destructor failure");
        }
    }
    let expected = controls();
    let cpu = executor(1, 1)?;
    for panic_on_drop in [false, true] {
        let (entered, observed) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let task = cpu.try_submit(move || {
            let _entered = entered.send(());
            let _released = wait.recv_timeout(Duration::from_secs(5));
            ForeignResult(panic_on_drop)
        })?;
        observed.recv_timeout(Duration::from_secs(5))?;
        drop(task);
        release.send(())?;
        // The single worker must finish publication cleanup before this kernel.
        let mut frame = FrameBatch::new(|value: &mut u32| *value = controls());
        let mut values = vec![0];
        frame.start(&cpu, &mut values)?;
        frame.reclaim(&mut values)?;
        assert_eq!(values, [expected]);
    }
    Ok(())
}
