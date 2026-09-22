//! Real-source scene requests refuse before growth and retain reusable cached inputs.
use super::*;
use crate::application::frame_pipeline::FrameWait;
use solarity_cpu::{CpuPoolConfig, CpuStoragePlan};
use solarity_rendering::{M2BonePose, M2BoneTransforms};
use std::{error::Error, num::NonZeroUsize};

impl ScenePoses {
    pub(in crate::application::terrain_frame::m2) fn assert_connected_admission(
        source: &M2GpuSource,
    ) -> Result<(), Box<dyn Error>> {
        let clock = M2AnimationClock::new(0, 120., 120.);
        let transforms = [(0, glam::Mat4::from_translation(glam::Vec3::Y))];
        let sequences = [(0, clock)];
        let overrides = M2BonePoseOverrides {
            model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
            bone_transforms: &transforms,
            bone_sequences: &sequences,
            ..Default::default()
        };
        for palette in [true, false] {
            let mut cpu = executor()?;
            let budget = cpu.storage().clone();
            let baseline = budget.snapshot().used(Class::Frame);
            let mut probe = Self::default();
            let mut output = M2BonePose::default();
            request(
                &mut probe,
                &cpu,
                source,
                clock,
                overrides,
                palette,
                &mut output,
            )?;
            let required = budget.snapshot().peak(Class::Frame) - baseline;
            drop((probe, output));
            assert_eq!(budget.snapshot().used(Class::Frame), baseline);
            let pressure = budget.reserve(
                Class::Frame,
                Kind::Scratch,
                budget.snapshot().limit(Class::Frame) - baseline - required + 1,
            )?;
            let held = budget.snapshot().used(Class::Frame);
            let mut poses = Self::default();
            let mut output = M2BonePose::default();
            assert!(matches!(
                request(
                    &mut poses,
                    &cpu,
                    source,
                    clock,
                    overrides,
                    palette,
                    &mut output
                ),
                Err(RuntimeTerrainFrameError::Cpu(
                    CpuError::StorageAtCapacity { .. }
                ))
            ));
            assert_eq!(poses.cache.capacity(), 0);
            assert_eq!(poses.indices.capacity(), 0);
            assert_eq!(poses.late_jobs.capacity(), 0);
            assert_eq!(budget.snapshot().used(Class::Frame), held);
            drop(pressure);
            // Full palettes exchange storage with the caller; warm both sides of that exchange.
            request(
                &mut poses,
                &cpu,
                source,
                clock,
                overrides,
                palette,
                &mut output,
            )?;
            request(
                &mut poses,
                &cpu,
                source,
                clock,
                overrides,
                palette,
                &mut output,
            )?;
            let pressure = budget.reserve(
                Class::Frame,
                Kind::Scratch,
                budget.snapshot().limit(Class::Frame) - budget.snapshot().used(Class::Frame),
            )?;
            let next = M2AnimationClock::new(0, 160., 160.);
            request(
                &mut poses,
                &cpu,
                source,
                next,
                overrides,
                palette,
                &mut output,
            )?;
            let mut serial = M2BonePose::default();
            serial.recompose_with_overrides(
                source.model.animations(),
                next,
                glam::Mat4::IDENTITY,
                overrides,
            )?;
            if palette {
                assert_eq!(output.transforms(), serial.transforms());
            } else {
                let samples = poses.sample(
                    Some(&cpu),
                    &mut FrameWait::Offline,
                    3,
                    source,
                    next,
                    glam::Mat4::IDENTITY,
                    overrides,
                    &[0],
                )?;
                assert_eq!(samples.bone_transform(0), serial.bone_transform(0));
            }
            let grown = [(0, glam::Mat4::IDENTITY); 16];
            let changed = M2BonePoseOverrides {
                bone_transforms: &grown,
                ..overrides
            };
            assert!(matches!(
                request(
                    &mut poses,
                    &cpu,
                    source,
                    next,
                    changed,
                    palette,
                    &mut output
                ),
                Err(RuntimeTerrainFrameError::Cpu(
                    CpuError::StorageAtCapacity { .. }
                ))
            ));
            assert!(
                poses.cache[3].is_some(),
                "refusal preserves the cached owner"
            );
            request(
                &mut poses,
                &cpu,
                source,
                next,
                overrides,
                palette,
                &mut output,
            )?;
            drop(pressure);
            poses.finish(&mut FrameWait::Offline)?;
            drop((poses, output));
            cpu.shutdown()?;
            assert_eq!(budget.snapshot().used(Class::Frame), baseline);
        }
        assert_seed_admission(source, clock, overrides)?;
        Ok(())
    }
}

fn executor() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        solarity_cpu::CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(1 << 20, 1 << 20, 0),
    ))
}

#[allow(clippy::too_many_arguments)]
fn request(
    poses: &mut ScenePoses,
    cpu: &CpuExecutor,
    source: &M2GpuSource,
    clock: M2AnimationClock,
    overrides: M2BonePoseOverrides<'_>,
    palette: bool,
    output: &mut M2BonePose,
) -> Result<(), RuntimeTerrainFrameError> {
    if palette {
        poses.palette(
            cpu,
            &mut FrameWait::Offline,
            3,
            source,
            clock,
            glam::Mat4::IDENTITY,
            overrides,
            output,
        )
    } else {
        poses
            .sample(
                Some(cpu),
                &mut FrameWait::Offline,
                3,
                source,
                clock,
                glam::Mat4::IDENTITY,
                overrides,
                &[0],
            )
            .map(|_| ())
    }
}

fn assert_seed_admission(
    source: &M2GpuSource,
    clock: M2AnimationClock,
    overrides: M2BonePoseOverrides<'_>,
) -> Result<(), Box<dyn Error>> {
    let mut cpu = executor()?;
    let budget = cpu.storage().clone();
    let baseline = budget.snapshot().used(Class::Frame);
    let mut probe = ScenePoses::default();
    probe.seed(
        &cpu,
        3,
        source,
        clock,
        glam::Mat4::IDENTITY,
        overrides,
        &[0],
    )?;
    let required = budget.snapshot().peak(Class::Frame) - baseline;
    drop(probe);
    let pressure = budget.reserve(
        Class::Frame,
        Kind::Scratch,
        budget.snapshot().limit(Class::Frame) - baseline - required + 1,
    )?;
    let held = budget.snapshot().used(Class::Frame);
    let mut poses = ScenePoses::default();
    assert!(
        poses
            .seed(
                &cpu,
                3,
                source,
                clock,
                glam::Mat4::IDENTITY,
                overrides,
                &[0]
            )
            .is_err()
    );
    assert_eq!(poses.cache.capacity(), 0);
    assert_eq!(poses.jobs.capacity(), 0);
    assert_eq!(budget.snapshot().used(Class::Frame), held);
    drop(pressure);
    poses.seed_sequences(
        &cpu,
        3,
        source,
        clock,
        glam::Mat4::IDENTITY,
        M2BonePoseOverrides {
            bone_sequences: &[],
            ..overrides
        },
        &[0],
        overrides.bone_sequences.iter().copied().filter(|_| true),
    )?;
    assert!(poses.seeded(3));
    poses.start(&cpu)?;
    let samples = poses.sample(
        Some(&cpu),
        &mut FrameWait::Offline,
        3,
        source,
        clock,
        glam::Mat4::IDENTITY,
        overrides,
        &[0],
    )?;
    let mut serial = M2BoneSamples::default();
    serial.recompose(
        source.model.animations(),
        clock,
        glam::Mat4::IDENTITY,
        overrides,
        &[0],
    )?;
    assert_eq!(samples.bone_transform(0), serial.bone_transform(0));
    assert_eq!(
        poses.hits, 1,
        "bounded iterator supplies the exact cached input"
    );
    poses.finish(&mut FrameWait::Offline)?;
    drop(poses);
    cpu.shutdown()?;
    assert_eq!(budget.snapshot().used(Class::Frame), baseline);
    Ok(())
}
