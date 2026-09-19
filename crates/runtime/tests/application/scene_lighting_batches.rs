//! Receiver fan-out must preserve stock indexing, late ancestry and failure prefixes.

use super::{LightingWork, RuntimeTerrainFrameError, SceneLighting};
use crate::application::frame_pipeline::FrameWait;
use glam::{Mat4, Vec3, Vec4};
use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, CpuStorageClass, CpuStoragePlan,
    FrameBatch, JobOutcome,
};
use solarity_rendering::{M2DirectionalLight, M2LocalLightState, M2PointLight, M2SceneUniform};
use std::{
    error::Error,
    num::NonZeroUsize,
    rc::Rc,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread::ThreadId,
    time::Duration,
};

/// The protected workers can prove independent progress while one receiver range is held.
fn executor(workers: usize, capacity: usize) -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::new(workers).ok_or(CpuError::BatchCapacity)?,
        NonZeroUsize::new(capacity).ok_or(CpuError::BatchCapacity)?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))
}

/// Fixed exterior facts isolate packet identity from camera animation.
fn uniforms() -> (M2SceneUniform, M2DirectionalLight) {
    (
        M2SceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
        M2DirectionalLight::new(-Vec3::Z, Vec3::splat(0.2), Vec3::ONE),
    )
}

/// Passenger zero references the last receiver, necessarily in a later range.
/// Unique fog values make any reordered/lost output observable in the final array.
fn populate(bank: &mut SceneLighting, count: usize) -> Result<Rc<()>, RuntimeTerrainFrameError> {
    let owner = Rc::new(());
    bank.sample_directional = vec![
        (0, M2DirectionalLight::new(Vec3::X, Vec3::Y, Vec3::Z)),
        (1, M2DirectionalLight::new(Vec3::Y, Vec3::Z, Vec3::X)),
    ];
    bank.sample_points = vec![
        M2PointLight::new(Vec3::ZERO, Vec3::splat(0.2), Vec3::X),
        M2PointLight::new(Vec3::X * 12., Vec3::splat(0.3), Vec3::Y),
    ];
    bank.publish(&owner, 0.75)?;
    for index in 0..count {
        bank.receiver_with_light(
            index,
            (index == 0 && count > 1).then_some(count - 1),
            Vec3::new(index as f32 * 0.125, 1., 2.),
            (index + 1 == count).then_some(M2DirectionalLight::new(Vec3::Z, Vec3::X, Vec3::Y)),
            Some(Vec3::splat(index as f32 * 0.001)),
        )?;
    }
    Ok(owner)
}

/// Metadata-only completion gate; the source owner survives until final reclamation.
fn ready(cpu: &CpuExecutor) -> Result<CompletionPort, CpuError> {
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    port.producer()?.complete(JobOutcome::Succeeded)?;
    Ok(port)
}

#[test]
fn partitioned_receivers_match_serial_with_late_parents_and_changing_counts()
-> Result<(), Box<dyn Error>> {
    for workers in [1, 4] {
        let cpu = executor(workers, 8)?;
        let mut bank = SceneLighting::default();
        let source_allocation = Arc::as_ptr(&bank.sources);
        let receiver_allocation = Arc::as_ptr(&bank.receivers);
        for count in [0, 1, 65, 513, 513, 64, 193, 0] {
            bank.clear();
            let _owner = populate(&mut bank, count)?;
            let mut reference = SceneLighting::default();
            let _reference_owner = populate(&mut reference, count)?;
            let (base, exterior) = uniforms();
            reference.finish(base, exterior)?;
            let port = ready(&cpu)?;
            bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior)?;
            bank.finish_pending(&mut FrameWait::Offline)?;
            assert_eq!(
                bank.scenes, reference.scenes,
                "workers={workers}, count={count}"
            );
            assert_eq!(Arc::as_ptr(&bank.sources), source_allocation);
            assert_eq!(Arc::as_ptr(&bank.receivers), receiver_allocation);
            assert_eq!(Arc::strong_count(&bank.sources), 1);
            assert_eq!(Arc::strong_count(&bank.receivers), 1);
        }
    }
    Ok(())
}

/// Only the controlled-concurrency test installs this alternate batch kernel.
struct HeldRange {
    events: mpsc::Sender<(usize, ThreadId)>,
    release: Arc<(Mutex<bool>, Condvar)>,
}
static HELD_RANGE: Mutex<Option<HeldRange>> = Mutex::new(None);

/// Always releases the held worker, including test assertion/receive failures.
struct ReleaseRange(Arc<(Mutex<bool>, Condvar)>);
impl Drop for ReleaseRange {
    fn drop(&mut self) {
        let (lock, changed) = &*self.0;
        *lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        changed.notify_all();
    }
}

/// Holds only the first real range; all later ranges execute the production operation.
fn held_first(job: &mut LightingWork) -> JobOutcome {
    let input = job
        .input
        .as_ref()
        .unwrap_or_else(|| unreachable!("test job owns input"));
    let start = input.range.start;
    let (events, release) = {
        let state = HELD_RANGE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = state
            .as_ref()
            .unwrap_or_else(|| unreachable!("test installs its gate"));
        (state.events.clone(), Arc::clone(&state.release))
    };
    if start == 0 {
        let _sent = events.send((start, std::thread::current().id()));
        let (lock, changed) = &*release;
        let mut released = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*released {
            released = changed
                .wait(released)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    let result = job.execute();
    if start != 0 {
        let _sent = events.send((start, std::thread::current().id()));
    }
    result
}

#[test]
fn later_receiver_ranges_complete_on_another_worker_before_first_publication()
-> Result<(), Box<dyn Error>> {
    let cpu = executor(3, 8)?;
    let mut bank = SceneLighting::default();
    let _owner = populate(&mut bank, 513)?;
    let mut reference = SceneLighting::default();
    let _reference_owner = populate(&mut reference, 513)?;
    let (base, exterior) = uniforms();
    reference.finish(base, exterior)?;
    let (events, receive) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    *HELD_RANGE.lock().map_err(|_| "test gate poisoned")? = Some(HeldRange {
        events,
        release: Arc::clone(&release),
    });
    let release = ReleaseRange(release);
    bank.batch.pending = FrameBatch::with_outcome(held_first);
    let port = ready(&cpu)?;
    bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior)?;
    let (mut first, mut later) = (None, None);
    while first.is_none() || later.is_none() {
        let (start, thread) = receive.recv_timeout(Duration::from_secs(10))?;
        if start == 0 {
            first = Some(thread);
        } else {
            later = Some(thread);
        }
    }
    assert_ne!(first, later, "independent ranges run on different workers");
    assert!(!bank.is_ready());
    assert!(
        bank.scenes.is_empty(),
        "completion cannot publish out of stock order"
    );
    assert_eq!(bank.inputs().points.points().len(), 2);
    drop(release);
    bank.finish_pending(&mut FrameWait::Offline)?;
    assert_eq!(bank.scenes, reference.scenes);
    *HELD_RANGE.lock().map_err(|_| "test gate poisoned")? = None;
    Ok(())
}

#[test]
fn failed_range_returns_only_serial_prefix_and_all_reader_pins() -> Result<(), Box<dyn Error>> {
    let cpu = executor(4, 8)?;
    let mut bank = SceneLighting::default();
    let _owner = populate(&mut bank, 193)?;
    // Replace only receivers, retaining the same immutable light sources.
    super::super::ReceiverInputs::exclusive(&mut bank.receivers).clear();
    for index in 0..193 {
        bank.receiver(
            index,
            None,
            if index == 70 || index == 150 {
                Vec3::splat(f32::NAN)
            } else {
                Vec3::ZERO
            },
        )?;
    }
    let port = ready(&cpu)?;
    let (base, exterior) = uniforms();
    bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior)?;
    assert!(matches!(
        bank.finish_pending(&mut FrameWait::Offline),
        Err(RuntimeTerrainFrameError::SceneLight(
            solarity_rendering::ScenePointLightError::InvalidBounds
        ))
    ));
    assert_eq!(bank.scenes.len(), 70);
    assert_eq!(Arc::strong_count(&bank.sources), 1);
    assert_eq!(Arc::strong_count(&bank.receivers), 1);
    bank.clear();
    Ok(())
}

#[test]
fn failed_dependency_returns_every_batch_pin_without_running_receivers()
-> Result<(), Box<dyn Error>> {
    let cpu = executor(4, 8)?;
    let mut bank = SceneLighting::default();
    let _owner = populate(&mut bank, 513)?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let (base, exterior) = uniforms();
    bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior)?;
    port.producer()?.complete(JobOutcome::Failed)?;
    assert!(matches!(
        bank.finish_pending(&mut FrameWait::Offline),
        Err(RuntimeTerrainFrameError::Cpu(CpuError::DependencyFailed))
    ));
    assert!(bank.scenes.is_empty());
    bank.clear();
    assert_eq!(Arc::strong_count(&bank.receivers), 1);
    assert_eq!(Arc::strong_count(&bank.sources), 1);
    Ok(())
}

#[test]
fn rejected_multi_range_admission_returns_all_inputs_for_retry() -> Result<(), Box<dyn Error>> {
    let cpu = executor(4, 1)?;
    let occupied_port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut occupied = FrameBatch::new(|_: &mut ()| {});
    occupied.begin_when(
        &cpu,
        solarity_cpu::FrameBatchPlan::new(1, 0),
        &occupied_port.readiness(),
    )?;
    occupied.push(&mut Some(()))?;
    occupied.close();
    let mut bank = SceneLighting::default();
    let _owner = populate(&mut bank, 513)?;
    let port = ready(&cpu)?;
    let (base, exterior) = uniforms();
    assert!(matches!(
        bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior),
        Err(RuntimeTerrainFrameError::Cpu(CpuError::AtCapacity { .. }))
    ));
    assert_eq!(Arc::strong_count(&bank.sources), 1);
    assert_eq!(Arc::strong_count(&bank.receivers), 1);
    occupied_port.producer()?.complete(JobOutcome::Succeeded)?;
    occupied.reclaim(&mut Vec::new())?;
    // Rebuild the same next frame after the explicit refusal; no reader pin is leaked.
    bank.clear();
    let _next_owner = populate(&mut bank, 513)?;
    bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior)?;
    bank.finish_pending(&mut FrameWait::Offline)?;
    assert_eq!(bank.scenes.len(), 513);
    Ok(())
}

/// Simulates an execution defect after admission, using the scheduler's unwind boundary.
fn panic_first(job: &mut LightingWork) -> JobOutcome {
    if job
        .input
        .as_ref()
        .is_some_and(|input| input.range.start == 0)
    {
        panic!("intentional first receiver kernel failure");
    }
    job.execute()
}

#[test]
fn worker_panic_returns_all_batch_pins_before_next_frame() -> Result<(), Box<dyn Error>> {
    let cpu = executor(4, 8)?;
    let mut bank = SceneLighting::default();
    let _owner = populate(&mut bank, 513)?;
    bank.batch.pending = FrameBatch::with_outcome(panic_first);
    let port = ready(&cpu)?;
    let (base, exterior) = uniforms();
    bank.begin_finish(Some(&cpu), Some(&port.readiness()), base, exterior)?;
    assert!(matches!(
        bank.finish_pending(&mut FrameWait::Offline),
        Err(RuntimeTerrainFrameError::Cpu(CpuError::TaskPanicked))
    ));
    assert!(bank.scenes.is_empty());
    assert_eq!(Arc::strong_count(&bank.sources), 1);
    assert_eq!(Arc::strong_count(&bank.receivers), 1);
    bank.clear();
    Ok(())
}
