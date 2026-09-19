//! Main queries share published sources while the receiver job is deliberately gated.

use crate::application::frame_pipeline::FrameWait;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::application::terrain_frame::m2::scene_lighting::SceneLighting;
use glam::{Mat4, Vec3, Vec4};
use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, CpuStorageClass, CpuStoragePlan,
    FrameBatch, FrameBatchPlan, JobOutcome,
};
use solarity_rendering::{M2DirectionalLight, M2LocalLightState, M2PointLight, M2SceneUniform};
use std::{error::Error, num::NonZeroUsize, rc::Rc};

/// One worker makes successful progress depend on readiness rather than spare lanes.
fn executor(capacity: usize) -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(capacity).ok_or(CpuError::BatchCapacity)?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))
}

/// Two model sources, one point and a receiver with its own exterior callback.
fn populated_bank() -> Result<(SceneLighting, Rc<()>), RuntimeTerrainFrameError> {
    let owner = Rc::new(());
    let mut bank = SceneLighting::default();
    bank.sample_directional = vec![
        (0, M2DirectionalLight::new(Vec3::X, Vec3::ZERO, Vec3::X)),
        (1, M2DirectionalLight::new(Vec3::Y, Vec3::ZERO, Vec3::Y)),
    ];
    bank.sample_points = vec![M2PointLight::new(Vec3::X, Vec3::ZERO, Vec3::ONE)];
    bank.publish(&owner, 1.0)?;
    bank.receiver_with_light(
        0,
        None,
        Vec3::ZERO,
        Some(M2DirectionalLight::new(Vec3::Z, Vec3::Y, Vec3::Z)),
        None,
    )?;
    Ok((bank, owner))
}

/// Fixed camera-independent uniform keeps the test focused on light ownership.
fn base() -> M2SceneUniform {
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
    )
}

#[test]
fn pending_receivers_leave_shared_surface_sources_readable_and_reusable()
-> Result<(), Box<dyn Error>> {
    let cpu = executor(8)?;
    let exterior = M2DirectionalLight::new(-Vec3::Z, Vec3::ZERO, Vec3::splat(0.25));
    let (mut reference, _reference_owner) = populated_bank()?;
    reference.finish(base(), exterior)?;
    let expected = reference
        .points()
        .terrain_lighting(Vec3::ZERO, 1.0, Vec3::Z * 5.0)?;
    let (mut bank, owner) = populated_bank()?;
    let allocation = bank.points() as *const _;
    for _ in 0..8 {
        let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
        let token = port.readiness();
        let mut producer = port.producer()?;
        bank.begin_finish(Some(&cpu), Some(&token), base(), exterior)?;
        assert!(bank.has_pending());
        assert!(!bank.is_ready());
        assert!(bank.scenes.is_empty());
        let inputs = bank.inputs();
        assert_eq!(
            inputs.points as *const _, allocation,
            "no copied light generation"
        );
        assert_eq!(inputs.directionals, reference.directionals());
        assert_eq!(
            inputs
                .points
                .terrain_lighting(Vec3::ZERO, 1.0, Vec3::Z * 5.0)?,
            expected
        );
        // A waiting receiver phase also leaves the only worker available.
        assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
        assert!(producer.complete(JobOutcome::Succeeded)?);
        bank.finish_pending(&mut FrameWait::Offline)?;
        assert_eq!(bank.scenes, reference.scenes);
        assert_eq!(
            bank.directionals(),
            reference.directionals(),
            "receiver exterior never alters shared sources"
        );
        bank.clear();
        assert_eq!(bank.points() as *const _, allocation);
        assert!(bank.points().points().is_empty());
        assert!(bank.directionals().is_empty());
        bank.publish(&owner, 1.0)?;
        bank.receiver_with_light(
            0,
            None,
            Vec3::ZERO,
            Some(M2DirectionalLight::new(Vec3::Z, Vec3::Y, Vec3::Z)),
            None,
        )?;
    }
    Ok(())
}

#[test]
fn failed_dependency_releases_shared_source_pin_before_next_frame() -> Result<(), Box<dyn Error>> {
    let cpu = executor(8)?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let token = port.readiness();
    let mut producer = port.producer()?;
    let (mut bank, _owner) = populated_bank()?;
    let allocation = bank.points() as *const _;
    bank.begin_finish(
        Some(&cpu),
        Some(&token),
        base(),
        M2DirectionalLight::new(-Vec3::Z, Vec3::ZERO, Vec3::ONE),
    )?;
    assert_eq!(bank.inputs().points.points().len(), 1);
    producer.complete(JobOutcome::Failed)?;
    assert!(matches!(
        bank.finish_pending(&mut FrameWait::Offline),
        Err(RuntimeTerrainFrameError::Cpu(CpuError::DependencyFailed))
    ));
    assert!(!bank.has_pending());
    bank.clear();
    assert_eq!(bank.points() as *const _, allocation);
    assert!(bank.points().points().is_empty());
    Ok(())
}

#[test]
fn rejected_receiver_admission_returns_source_ownership() -> Result<(), Box<dyn Error>> {
    let cpu = executor(1)?;
    let port = CompletionPort::new(2, cpu.storage(), CpuStorageClass::Frame)?;
    let token = port.readiness();
    let mut producer = port.producer()?;
    let mut occupied = FrameBatch::new(|_: &mut ()| {});
    occupied.begin_when(&cpu, FrameBatchPlan::new(1, 0), &token)?;
    occupied.push(&mut Some(()))?;
    occupied.close();
    let (mut bank, _owner) = populated_bank()?;
    let allocation = bank.points() as *const _;
    assert!(matches!(
        bank.begin_finish(
            Some(&cpu),
            Some(&token),
            base(),
            M2DirectionalLight::new(-Vec3::Z, Vec3::ZERO, Vec3::ONE)
        ),
        Err(RuntimeTerrainFrameError::Cpu(CpuError::AtCapacity { .. }))
    ));
    assert!(!bank.has_pending());
    assert_eq!(bank.points().points().len(), 1);
    bank.clear();
    assert_eq!(bank.points() as *const _, allocation);
    producer.complete(JobOutcome::Succeeded)?;
    occupied.reclaim(&mut Vec::new())?;
    Ok(())
}

#[test]
fn receiver_failure_restores_partial_output_and_releases_source_pin() -> Result<(), Box<dyn Error>>
{
    let cpu = executor(8)?;
    let port = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let token = port.readiness();
    port.producer()?.complete(JobOutcome::Succeeded)?;
    let (mut bank, _owner) = populated_bank()?;
    let allocation = bank.points() as *const _;
    bank.receiver(1, None, Vec3::splat(f32::NAN))?;
    bank.begin_finish(
        Some(&cpu),
        Some(&token),
        base(),
        M2DirectionalLight::new(-Vec3::Z, Vec3::ZERO, Vec3::ONE),
    )?;
    assert!(matches!(
        bank.finish_pending(&mut FrameWait::Offline),
        Err(RuntimeTerrainFrameError::SceneLight(
            solarity_rendering::ScenePointLightError::InvalidBounds
        ))
    ));
    assert_eq!(
        bank.scenes.len(),
        1,
        "successful predecessor output returns on failure"
    );
    assert!(!bank.has_pending());
    bank.clear();
    assert_eq!(bank.points() as *const _, allocation);
    assert!(bank.scenes.is_empty());
    Ok(())
}
