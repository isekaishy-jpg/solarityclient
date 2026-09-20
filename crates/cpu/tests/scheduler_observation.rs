//! Real publication and mutex observations accompany owned frame completion.

use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch};
use solarity_profiling::{Capture, begin_frame};
use std::{error::Error, num::NonZeroUsize};

use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    time::Duration,
};

/// Each worker holds one input until the coordinator observes the whole admitted
/// width. Release precedes reporting failure so a missed wake cannot hang cleanup.
struct HeldInput {
    entered: mpsc::Sender<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
    completed: bool,
}

/// Controlled blocking is fixture-only, exposing every admitted runner at once.
fn hold(input: &mut HeldInput) {
    let _observed = input.entered.send(());
    let (lock, ready) = &*input.release;
    let released = lock
        .lock()
        .unwrap_or_else(|_| unreachable!("fixture metadata"));
    let _released = ready
        .wait_while(released, |released| !*released)
        .unwrap_or_else(|_| unreachable!("fixture metadata"));
    input.completed = true;
}

#[test]
fn one_publication_wakes_every_reserved_runner_and_returns_all_inputs() -> Result<(), Box<dyn Error>>
{
    for workers in [1, 3, 5, 7] {
        let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
            CpuExecutionPlan::new(workers - 1, 1, 1, 1)?,
            NonZeroUsize::new(8).ok_or("capacity")?,
            CpuStoragePlan::new(4 << 20, 4 << 20, 0),
        ))?;
        let mut batch = FrameBatch::new(hold);
        for _ in 0..4 {
            let (entered, observed) = mpsc::channel();
            let release = Arc::new((Mutex::new(false), Condvar::new()));
            let mut inputs = (0..workers)
                .map(|_| HeldInput {
                    entered: entered.clone(),
                    release: Arc::clone(&release),
                    completed: false,
                })
                .collect();
            batch.start(&cpu, &mut inputs)?;
            batch.close();
            let all_entered =
                (0..workers).all(|_| observed.recv_timeout(Duration::from_secs(5)).is_ok());
            *release
                .0
                .lock()
                .unwrap_or_else(|_| unreachable!("fixture metadata")) = true;
            release.1.notify_all();
            batch.wait_until_finished()?;
            batch.reclaim(&mut inputs)?;
            assert!(all_entered, "not all {workers} runners became eligible");
            assert!(inputs.iter().all(|input| input.completed));
        }
        cpu.shutdown()?;
    }
    Ok(())
}

/// Multiple runner reservations execute all inputs and publish sampled queue and
/// ownership intervals. Wake observations are conditional on actual native parking.
#[test]
fn frame_publication_reports_separate_queue_and_ownership_intervals() -> Result<(), Box<dyn Error>>
{
    let root = std::env::temp_dir().join(format!("solarity-dispatch-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=dispatch".to_owned());
    let (_, path) = capture.toggle()?;
    let frame = begin_frame();
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(4, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(4 << 20, 4 << 20, 0),
    ))?;
    let mut batch = FrameBatch::new(|value: &mut usize| *value += 1);
    let mut inputs = vec![0; 64];
    for _ in 0..8 {
        batch.start(&cpu, &mut inputs)?;
        batch.close();
        batch.wait_until_finished()?;
        batch.reclaim(&mut inputs)?;
    }
    assert_eq!(inputs, vec![8; 64]);
    let mut turns = 0;
    let service = cpu.try_reserve()?.submit_steps(move || {
        turns += 1;
        if turns == 3 {
            std::ops::ControlFlow::Break(turns)
        } else {
            std::ops::ControlFlow::Continue(())
        }
    });
    assert_eq!(service.join()?, 3);
    cpu.shutdown()?;
    drop(frame);
    capture.shutdown()?;
    let summary = std::fs::read_to_string(path.with_extension("summary.csv"))?;
    for name in [
        "cpu.dispatch.frame.queue_wait",
        "cpu.dispatch.service.queue_wait",
        "cpu.dispatch.lock_wait",
        "cpu.batch.lock_wait",
    ] {
        assert!(summary.contains(name), "missing {name}");
    }
    Ok(())
}
