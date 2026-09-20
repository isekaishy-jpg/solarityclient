//! Scoped scratch and cancellation retain inputs across every terminal outcome.

use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuScratch, CpuService,
    CpuStorageClass, CpuStoragePlan, FrameBatch, FrameBatchPlan, JobContext, JobIdentity,
    JobOutcome, LoadBatch,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

/// Tracks temporary value destruction separately from reusable allocation ownership.
struct Temporary(Arc<AtomicUsize>);
impl Drop for Temporary {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Controlled handshakes place cancellation inside an already executing kernel.
struct Input {
    scratch: CpuScratch<Temporary>,
    drops: Arc<AtomicUsize>,
    started: Option<mpsc::Sender<()>>,
    resume: Option<mpsc::Receiver<()>>,
    identity: Option<JobIdentity>,
    cancelled: bool,
    unwind: bool,
    output: u32,
}

/// No borrowed temporary is copied into the output; only the computed scalar survives.
fn execute(input: &mut Input, context: &JobContext<'_>) -> JobOutcome {
    input.identity = Some(context.identity());
    context.diagnostic_value("test.context.output", 42);
    let mut scratch = context.scratch(&mut input.scratch);
    if !scratch.writer().is_empty()
        || scratch
            .writer()
            .push(Temporary(Arc::clone(&input.drops)))
            .is_err()
    {
        return JobOutcome::Failed;
    }
    if let Some(started) = &input.started {
        let _sent = started.send(());
    }
    if let Some(resume) = &input.resume
        && resume.recv_timeout(Duration::from_secs(5)).is_err()
    {
        return JobOutcome::Failed;
    }
    input.cancelled = context.is_cancelled();
    if input.cancelled {
        return JobOutcome::Cancelled;
    }
    if input.unwind {
        std::panic::resume_unwind(Box::new("controlled context kernel unwind"));
    }
    input.output = 42;
    JobOutcome::Succeeded
}

/// A single service lane makes the fixture deterministic without machine discovery.
fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(1, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

/// Scratch is charged before transferring the owned input, never during execution.
fn input(cpu: &CpuExecutor, class: CpuStorageClass) -> Result<Input, CpuError> {
    let mut scratch = CpuScratch::default();
    scratch.reserve(cpu.storage(), class, 2)?;
    Ok(Input {
        scratch,
        drops: Arc::new(AtomicUsize::new(0)),
        started: None,
        resume: None,
        identity: None,
        cancelled: false,
        unwind: false,
        output: 0,
    })
}

/// Reusing an epoch changes provenance while keeping admitted bytes and scratch capacity stable.
#[test]
fn warmed_context_reuses_admitted_scratch_and_resets_values() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut inputs = vec![input(&cpu, CpuStorageClass::Frame)?];
    let drops = Arc::clone(&inputs[0].drops);
    let mut batch = FrameBatch::with_context(execute);
    let mut previous_epoch = 0;
    let mut retained_bytes = None;
    for iteration in 1..=32 {
        batch.start(&cpu, &mut inputs)?;
        batch.reclaim(&mut inputs)?;
        let identity = inputs[0].identity.ok_or("missing identity")?;
        assert!(identity.epoch() > previous_epoch);
        assert_eq!(identity.index(), 0);
        assert_eq!(inputs[0].output, 42);
        assert_eq!(inputs[0].scratch.capacity(), 2);
        assert_eq!(drops.load(Ordering::SeqCst), iteration);
        let bytes = cpu.storage().snapshot().used(CpuStorageClass::Frame);
        assert_eq!(*retained_bytes.get_or_insert(bytes), bytes);
        previous_epoch = identity.epoch();
    }
    Ok(())
}

/// Running cancellation is observed before return without taking the scheduler lock.
#[test]
fn frame_and_loading_contexts_observe_withdrawal_and_return_owned_inputs()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    for loading in [false, true] {
        let mut value = input(
            &cpu,
            if loading {
                CpuStorageClass::Required
            } else {
                CpuStorageClass::Frame
            },
        )?;
        let (started, observed) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        value.started = Some(started);
        value.resume = Some(resume);
        let drops = Arc::clone(&value.drops);
        let mut returned = Vec::new();
        let result = if loading {
            let mut batch = LoadBatch::with_context(CpuService::Required, execute);
            returned.push(value);
            batch.start_after(&cpu, &mut returned, &[])?;
            observed.recv_timeout(Duration::from_secs(5))?;
            batch.cancel();
            release.send(())?;
            batch.reclaim(&mut returned)
        } else {
            let mut batch = FrameBatch::with_context(execute);
            batch.begin(&cpu, FrameBatchPlan::new(1, 0))?;
            let handle = batch.push(&mut Some(value))?;
            batch.close();
            observed.recv_timeout(Duration::from_secs(5))?;
            batch.cancel(&handle)?;
            release.send(())?;
            batch.reclaim(&mut returned)
        };
        assert!(matches!(result, Err(CpuError::JobCancelled)));
        assert_eq!(returned.len(), 1);
        assert!(returned[0].cancelled);
        assert_eq!(returned[0].output, 0);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
    Ok(())
}

/// The context loan clears temporaries during unwind before the retained input returns.
#[test]
fn kernel_unwind_clears_scratch_and_allows_next_epoch() -> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    let mut value = input(&cpu, CpuStorageClass::Frame)?;
    value.unwind = true;
    let drops = Arc::clone(&value.drops);
    let mut inputs = vec![value];
    let mut batch = FrameBatch::with_context(execute);
    batch.start(&cpu, &mut inputs)?;
    assert!(matches!(
        batch.reclaim(&mut inputs),
        Err(CpuError::TaskPanicked)
    ));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    inputs[0].unwind = false;
    batch.start(&cpu, &mut inputs)?;
    batch.reclaim(&mut inputs)?;
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert_eq!(inputs[0].output, 42);
    Ok(())
}
