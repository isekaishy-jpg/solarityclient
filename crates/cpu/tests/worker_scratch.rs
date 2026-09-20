//! Worker-lane ownership survives replacement, cancellation and kernel unwind.

use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStorageClass as Class,
    CpuStorageKind as Kind, CpuStoragePlan, CpuWorkerScratch, FrameBatch, JobContext, JobOutcome,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    ops::ControlFlow,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

/// Controlled worker counts and allowances avoid host-dependent execution policy.
fn cpu(workers: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, workers, 1, workers)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

#[test]
fn version_growth_refusal_and_trim_preserve_pinned_storage() -> Result<(), Box<dyn Error>> {
    let cpu = cpu(2)?;
    let before = cpu.storage().snapshot().bytes(Class::Frame, Kind::Scratch);
    let mut scratch = CpuWorkerScratch::<u64>::new(&cpu, Class::Frame)?;
    scratch.reserve(8)?;
    assert_eq!(
        cpu.storage().snapshot().bytes(Class::Frame, Kind::Scratch),
        before + 128
    );
    let old = scratch.clone();
    scratch.reserve(32)?;
    assert_eq!(old.capacity(), 8);
    assert_eq!(scratch.capacity(), 32);
    assert_eq!(
        cpu.storage().snapshot().bytes(Class::Frame, Kind::Scratch),
        before + 128 + 512
    );
    let charged = cpu.storage().snapshot().used(Class::Frame);
    assert!(scratch.reserve(1 << 20).is_err());
    assert_eq!(scratch.capacity(), 32);
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), charged);
    scratch.trim(4)?;
    assert_eq!(
        cpu.storage().snapshot().bytes(Class::Frame, Kind::Scratch),
        before + 128 + 64
    );
    let task = cpu.try_reserve()?.submit_with_context(move |context| {
        context.with_worker_scratch(&old, 8, |scope| scope.writer().extend_from_slice(&[7; 8]))
    });
    task.join()???;
    drop(scratch);
    assert_eq!(
        cpu.storage().snapshot().bytes(Class::Frame, Kind::Scratch),
        before
    );
    Ok(())
}

#[test]
fn foreign_executor_capacity_and_nested_loans_are_rejected() -> Result<(), Box<dyn Error>> {
    let first = cpu(1)?;
    let second = cpu(1)?;
    let mut scratch = CpuWorkerScratch::<u32>::new(&first, Class::Frame)?;
    scratch.reserve(4)?;
    assert!(scratch.belongs_to(&first));
    assert!(!scratch.belongs_to(&second));
    let foreign = scratch.clone();
    let denied = second
        .try_reserve()?
        .submit_with_context(move |context| context.with_worker_scratch(&foreign, 1, |_| ()))
        .join()?;
    assert!(matches!(denied, Err(CpuError::WorkerScratchOwner)));
    first
        .try_reserve()?
        .submit_with_context(move |context| -> Result<(), CpuError> {
            assert!(matches!(
                context.with_worker_scratch(&scratch, 5, |_| ()),
                Err(CpuError::OutputCapacity { .. })
            ));
            context.with_worker_scratch(&scratch, 4, |scope| {
                scope.writer().push(17)?;
                assert!(matches!(
                    context.with_worker_scratch(&scratch, 1, |_| ()),
                    Err(CpuError::WorkerScratchBorrowed)
                ));
                assert_eq!(&scope.writer()[..], &[17]);
                Ok::<_, CpuError>(())
            })??;
            context.with_worker_scratch(&scratch, 4, |scope| assert!(scope.writer().is_empty()))?;
            Ok(())
        })
        .join()??;
    Ok(())
}

/// A temporary proves destruction happens before the next operation borrows a lane.
struct Temporary(Arc<AtomicUsize>);
impl Drop for Temporary {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn unwind_returns_empty_scratch_and_service_steps_reuse_it() -> Result<(), Box<dyn Error>> {
    let cpu = cpu(1)?;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut scratch = CpuWorkerScratch::<Temporary>::new(&cpu, Class::Frame)?;
    scratch.reserve(2)?;
    let failing = scratch.clone();
    let count = Arc::clone(&drops);
    let task = cpu
        .try_reserve()?
        .submit_with_context(move |context| -> Result<(), CpuError> {
            context.with_worker_scratch(&failing, 1, |scope| {
                assert!(scope.writer().push(Temporary(count)).is_ok());
                std::panic::resume_unwind(Box::new("controlled scratch unwind"));
            })
        });
    assert!(matches!(task.join(), Err(CpuError::TaskPanicked)));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let warm = scratch.clone();
    let count = Arc::clone(&drops);
    let mut turns = 0;
    cpu.try_reserve()?
        .submit_steps_with_context(move |context| {
            assert!(
                context
                    .with_worker_scratch(&warm, 2, |scope| {
                        assert!(scope.writer().is_empty());
                        assert!(scope.writer().push(Temporary(Arc::clone(&count))).is_ok());
                    })
                    .is_ok()
            );
            turns += 1;
            if turns == 3 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .join()?;
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    assert_eq!(scratch.worker_peaks().collect::<Vec<_>>(), [2]);
    Ok(())
}

/// Controlled simultaneous loans prove independent workers never alias storage.
struct Parallel {
    scratch: CpuWorkerScratch<u64>,
    ready: mpsc::Sender<usize>,
    resume: mpsc::Receiver<()>,
    address: usize,
}
fn execute(value: &mut Parallel, context: &JobContext<'_>) -> JobOutcome {
    let result = context.with_worker_scratch(&value.scratch, 8, |scope| {
        assert!(scope.writer().is_empty());
        assert!(scope.writer().push(23).is_ok());
        value.address = scope.writer().as_ptr().addr();
        assert!(value.ready.send(value.address).is_ok());
        assert!(value.resume.recv_timeout(Duration::from_secs(5)).is_ok());
        assert_eq!(scope.writer()[0], 23);
    });
    if result.is_ok() {
        JobOutcome::Succeeded
    } else {
        JobOutcome::Failed
    }
}

#[test]
fn simultaneous_workers_use_distinct_lanes_and_cancellation_returns_them()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu(2)?;
    let mut scratch = CpuWorkerScratch::new(&cpu, Class::Frame)?;
    scratch.reserve(8)?;
    let (ready, observed) = mpsc::channel();
    let mut release = Vec::new();
    let mut inputs = Vec::new();
    for _ in 0..2 {
        let (sender, resume) = mpsc::channel();
        release.push(sender);
        inputs.push(Parallel {
            scratch: scratch.clone(),
            ready: ready.clone(),
            resume,
            address: 0,
        });
    }
    let mut batch = FrameBatch::with_context(execute);
    batch.start(&cpu, &mut inputs)?;
    let first = observed.recv_timeout(Duration::from_secs(5))?;
    let second = observed.recv_timeout(Duration::from_secs(5))?;
    assert_ne!(first, second);
    for index in 0..2 {
        let handle = batch.job(index)?;
        batch.cancel(&handle)?;
    }
    for sender in release {
        sender.send(())?;
    }
    assert!(matches!(
        batch.reclaim(&mut inputs),
        Err(CpuError::JobCancelled)
    ));
    assert_eq!(inputs.len(), 2);
    assert_eq!(scratch.worker_peaks().collect::<Vec<_>>(), [8, 8]);
    // Both containers have returned even though terminal publication was cancelled.
    for _ in 0..4 {
        let scratch = scratch.clone();
        cpu.try_reserve()?
            .submit_with_context(move |context| {
                context.with_worker_scratch(&scratch, 8, |scope| assert!(scope.writer().is_empty()))
            })
            .join()??;
    }
    Ok(())
}
