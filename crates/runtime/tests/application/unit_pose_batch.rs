//! Worker sampling preserves moving-camera palettes and rejects stale inputs.

use super::*;
use crate::test_support::{ClientFixture, game_object_models};
use glam::Vec3;
use solarity_asset::ResourceLease;
use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use std::error::Error;
use std::num::NonZeroUsize;
use std::time::Instant;

// Test inspection stays outside production implementations.
impl super::super::PoseBatch {
    pub(in crate::application::terrain_frame::m2) fn named_consumption(&self) -> (usize, usize) {
        (
            self.jobs.iter().filter(|job| job.sparse).count(),
            self.jobs
                .iter()
                .filter(|job| job.sparse && job.result.is_none())
                .count(),
        )
    }

    pub(in crate::application::terrain_frame::m2) fn consumption(&self) -> (usize, usize) {
        (
            self.jobs.len(),
            self.jobs.iter().filter(|job| job.result.is_none()).count(),
        )
    }
}

fn executor() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(4).ok_or("worker count")?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(8).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

fn model() -> Result<ResourceLease<DecodedM2Model>, Box<dyn Error>> {
    let mut bytes = game_object_models::model_with_animations(&[0, 15])?;
    let bone = u32::from_le_bytes(bytes[0x30..0x34].try_into()?) as usize;
    bytes[bone + 4..bone + 8].copy_from_slice(&0x208_u32.to_le_bytes());
    let keys = bytes.len() as u32;
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes[0x34..0x38].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x38..0x3c].copy_from_slice(&keys.to_le_bytes());
    let times = bytes.len() as u32;
    for time in [0_u32, 1000] {
        bytes.extend_from_slice(&time.to_le_bytes());
    }
    let values = bytes.len() as u32;
    for value in [0_f32, 0., 0., 4., 2., 1.] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let time_channel = bytes.len() as u32;
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&times.to_le_bytes());
    let value_channel = bytes.len() as u32;
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&values.to_le_bytes());
    bytes[bone + 16..bone + 18].copy_from_slice(&1_u16.to_le_bytes());
    for (offset, value) in [
        (20, 1_u32),
        (24, time_channel),
        (28, 1),
        (32, value_channel),
    ] {
        bytes[bone + offset..bone + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[("Pose.m2", &bytes), ("Pose00.skin", &skin)])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(ResourceLease::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Pose.m2")?,
    )?))
}

fn overrides(job: &PoseJob) -> M2BonePoseOverrides<'_> {
    M2BonePoseOverrides {
        model_oriented_billboard_bones: &job.orientation,
        finger_pose: job.fingers,
        bone_transforms: &job.transforms,
        bone_sequences: &job.sequences,
    }
}

#[test]
fn worker_unit_poses_match_serial_during_camera_and_override_changes() -> Result<(), Box<dyn Error>>
{
    let model = model()?;
    let cpu = executor()?;
    let mut jobs: Vec<_> = (0..16)
        .map(|_| PoseJob::new(ResourceLease::clone(&model)))
        .collect();
    let mut serial = M2BonePose::default();
    let mut output = M2BonePose::default();
    for frame in 0..12 {
        for (index, job) in jobs.iter_mut().enumerate() {
            let tick = (frame * 31 + index * 7) as f32;
            job.clock = M2AnimationClock::new(0, tick, tick);
            job.view = Mat4::from_rotation_y(tick * 0.001)
                * Mat4::from_translation(Vec3::X * index as f32);
            job.transforms = vec![(0, Mat4::from_translation(Vec3::Y * frame as f32))];
            job.sequences = vec![(0, M2AnimationClock::new(1, tick * 0.5, tick))];
            job.orientation = vec![frame % 2 == 0];
        }
        {
            let mut batch = solarity_cpu::FrameBatch::new(PoseJob::sample);
            // Unequal hints force a different execution order for the same exact
            // skeletal inputs; publication must retain model/index correspondence.
            let costs: Vec<_> = (0..jobs.len())
                .map(|index| {
                    solarity_cpu::JobCost::measured(std::time::Duration::from_micros(
                        [10, 400, 100][index % 3],
                    ))
                })
                .collect();
            batch.start_costed_graph(
                &cpu,
                &solarity_cpu::FrameGraphTemplate::independent(jobs.len()),
                &mut jobs,
                &[],
                &costs,
            )?;
            batch.reclaim(&mut jobs)?;
        }
        for job in &mut jobs {
            serial.recompose_with_overrides(
                model.animations(),
                job.clock,
                job.view,
                overrides(job),
            )?;
            assert_eq!(job.pose.transforms(), serial.transforms());
            let orientation = job.orientation.clone();
            let transforms = job.transforms.clone();
            let sequences = job.sequences.clone();
            let input = M2BonePoseOverrides {
                model_oriented_billboard_bones: &orientation,
                bone_transforms: &transforms,
                bone_sequences: &sequences,
                finger_pose: job.fingers,
            };
            assert!(job.take(&model, job.clock, job.view, input, &mut output)?);
            assert_eq!(output.transforms(), serial.transforms());
            assert!(!job.take(&model, job.clock, job.view, input, &mut output)?);
        }
    }
    Ok(())
}

#[test]
fn prepared_unit_pose_rejects_every_changed_dependency() -> Result<(), Box<dyn Error>> {
    let model = model()?;
    let mut job = PoseJob::new(ResourceLease::clone(&model));
    job.sample();
    let mut output = M2BonePose::default();
    let clock = job.clock;
    let view = job.view;
    let changed_clock = M2AnimationClock::new(0, 1., 1.);
    let transforms = [(0, Mat4::IDENTITY)];
    let sequences = [(0, changed_clock)];
    let masks = [true];
    for (clock, view, input) in [
        (changed_clock, view, M2BonePoseOverrides::default()),
        (
            clock,
            Mat4::from_rotation_x(0.5),
            M2BonePoseOverrides::default(),
        ),
        (
            clock,
            view,
            M2BonePoseOverrides {
                finger_pose: Some((clock, M2FingerPoseHands::Both)),
                ..Default::default()
            },
        ),
        (
            clock,
            view,
            M2BonePoseOverrides {
                bone_transforms: &transforms,
                ..Default::default()
            },
        ),
        (
            clock,
            view,
            M2BonePoseOverrides {
                bone_sequences: &sequences,
                ..Default::default()
            },
        ),
        (
            clock,
            view,
            M2BonePoseOverrides {
                model_oriented_billboard_bones: &masks,
                ..Default::default()
            },
        ),
    ] {
        assert!(!job.take(&model, clock, view, input, &mut output)?);
    }
    let replacement = self::model()?;
    assert!(!job.take(
        &replacement,
        clock,
        view,
        M2BonePoseOverrides::default(),
        &mut output
    )?);
    assert!(job.take(
        &model,
        clock,
        view,
        M2BonePoseOverrides::default(),
        &mut output
    )?);
    job.view = Mat4::ZERO;
    job.sample();
    assert!(
        job.take(
            &model,
            clock,
            Mat4::ZERO,
            M2BonePoseOverrides::default(),
            &mut output
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn worker_named_bones_match_serial_and_reject_changed_demand() -> Result<(), Box<dyn Error>> {
    use solarity_rendering::M2BoneTransforms;
    let model = model()?;
    let cpu = executor()?;
    let mut jobs: Vec<_> = (0..16)
        .map(|_| PoseJob::new(ResourceLease::clone(&model)))
        .collect();
    let mut serial = M2BoneSamples::default();
    let mut output = M2BoneSamples::default();
    let mut palette = M2BonePose::default();
    for frame in 0..8 {
        for (index, job) in jobs.iter_mut().enumerate() {
            let tick = (frame * 31 + index * 7) as f32;
            job.clock = M2AnimationClock::new(0, tick, tick);
            job.view = Mat4::from_rotation_y(tick * 0.001);
            job.transforms = vec![(0, Mat4::from_translation(Vec3::Y * frame as f32))];
            job.request_samples(&[0], cpu.storage())?;
            job.admit(&cpu)?;
        }
        let mut batch = solarity_cpu::FrameBatch::with_context(PoseJob::execute);
        batch.start(&cpu, &mut jobs)?;
        batch.reclaim(&mut jobs)?;
        for job in &mut jobs {
            let transforms = job.transforms.clone();
            let input = M2BonePoseOverrides {
                bone_transforms: &transforms,
                ..Default::default()
            };
            serial.recompose(model.animations(), job.clock, job.view, input, &[0])?;
            assert!(
                !job.take(&model, job.clock, job.view, input, &mut palette)?,
                "named output cannot become a render palette"
            );
            assert!(
                !job.take_samples(&model, job.clock, job.view, input, &[], &mut output)?,
                "changed callback demand rejects the prepared result"
            );
            assert!(!job.take_samples(&model, job.clock, Mat4::ZERO, input, &[0], &mut output)?);
            assert!(job.take_samples(&model, job.clock, job.view, input, &[0], &mut output)?);
            assert_eq!(output.bone_transform(0), serial.bone_transform(0));
            assert!(!job.take_samples(&model, job.clock, job.view, input, &[0], &mut output)?);
        }
    }
    Ok(())
}

/// Explicit CPU-only benchmark. No window, server, GPU, or character movement.
#[test]
#[ignore = "requires SOLARITY_DATA_ROOT and an optimized test-client build"]
fn benchmark_moving_unit_pose_batch() -> Result<(), Box<dyn Error>> {
    let root = std::env::var("SOLARITY_DATA_ROOT")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let mut models = Vec::new();
    for path in [
        "Character/Orc/Male/OrcMale.m2",
        "Character/Tauren/Male/TaurenMale.m2",
        "Character/Troll/Male/TrollMale.m2",
        "Character/Human/Male/HumanMale.m2",
    ] {
        models.push(ResourceLease::new(DecodedM2Model::load(
            &mut store,
            &AssetPath::new(path)?,
        )?));
    }
    let mut jobs: Vec<_> = (0..120)
        .map(|index| PoseJob::new(ResourceLease::clone(&models[index % models.len()])))
        .collect();
    let cpu = executor()?;
    let mut totals = [std::time::Duration::ZERO; 2];
    let mut serial = M2BonePose::default();
    for frame in 0..300 {
        for (index, job) in jobs.iter_mut().enumerate() {
            let tick = (frame * 11 + index * 17) as f32;
            let sequence = job
                .model
                .animations()
                .sequence_for_variation(if index % 2 == 0 { 0 } else { 4 }, 0)
                .ok_or("benchmark sequence")?;
            job.clock = M2AnimationClock::new(sequence, tick % 900., tick);
            job.view = Mat4::from_rotation_z(frame as f32 * 0.001)
                * Mat4::from_translation(Vec3::new(index as f32, 20., 0.));
        }
        // Alternate order within each frame to expose scheduling/thermal bias.
        for parallel in [frame % 2 == 0, frame % 2 != 0] {
            let start = Instant::now();
            if parallel {
                {
                    let mut batch = solarity_cpu::FrameBatch::new(PoseJob::sample);
                    batch.start(&cpu, &mut jobs)?;
                    batch.reclaim(&mut jobs)?;
                }
            } else {
                for job in &jobs {
                    serial.recompose_with_overrides(
                        job.model.animations(),
                        job.clock,
                        job.view,
                        overrides(job),
                    )?;
                    std::hint::black_box(serial.transforms());
                }
            }
            if frame >= 20 {
                totals[usize::from(parallel)] += start.elapsed();
            }
        }
        for job in &jobs {
            serial.recompose_with_overrides(
                job.model.animations(),
                job.clock,
                job.view,
                overrides(job),
            )?;
            assert_eq!(job.pose.transforms(), serial.transforms());
        }
    }
    println!(
        "120 moving units, 280 paired frames: serial={:.4} ms batch={:.4} ms ratio={:.2}; exact palettes verified",
        totals[0].as_secs_f64() * 1000. / 280.,
        totals[1].as_secs_f64() * 1000. / 280.,
        totals[0].as_secs_f64() / totals[1].as_secs_f64()
    );
    Ok(())
}
