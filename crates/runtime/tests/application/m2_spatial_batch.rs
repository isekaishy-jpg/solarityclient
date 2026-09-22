//! Native admission gates, per-owner failure order and owned-phase reuse.

use super::SpatialBatch;
use super::input::{SpatialView, StaticAdmissionInput};
use crate::application::frame_pipeline::FrameWait;
use crate::application::m2_spatial::StaticM2Spatial;
use crate::application::terrain_frame::shadow::{ModelShadowKind, WorldShadowAdmission};
use glam::{Mat4, Vec3};
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use solarity_rendering::{
    WorldCamera, WorldEnvironmentShadowFrame, WorldEnvironmentShadowState, WorldFrustum,
    WorldScreenWindow, WorldShadowProjection, WorldShadowQuality,
};
use std::{error::Error, num::NonZeroUsize};

/// No native window or installed archives are needed for pure collector tests.
fn executor() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::new(4).ok_or("workers")?;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// The same native cascade volumes as environment_shadow_admission.rs.
fn view() -> Result<SpatialView, Box<dyn Error>> {
    let camera = WorldCamera::orthographic(
        Vec3::Z * 100.,
        Vec3::ZERO,
        Vec3::Y,
        [-1000., 1000.],
        [-1000., 1000.],
        0.1,
        200.,
    )
    .frame(1.)?;
    let quality = WorldShadowQuality::Cascaded;
    let mut state = WorldEnvironmentShadowState::new(quality);
    let updates = state.advance(Vec3::ZERO)?;
    let primary =
        WorldShadowProjection::primary(quality, Vec3::ZERO, camera.camera().position(), -Vec3::Z)?
            .with_camera_culling(camera);
    let environment =
        WorldEnvironmentShadowFrame::new(&state, updates, camera.camera().position(), -Vec3::Z)?;
    Ok(SpatialView {
        // Distance and frustum eyes are separate here to exercise the native
        // distance boundaries without moving the exact tested cascade volumes.
        camera: Vec3::ZERO,
        detail: 1.,
        frustum: WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        shadows: Some(WorldShadowAdmission::new(
            primary,
            environment,
            camera,
            -Vec3::Z,
        )?),
    })
}

/// A size-class-three authored box admits ordinary scenery up to 750 units.
fn input(position: Vec3) -> StaticAdmissionInput {
    StaticAdmissionInput {
        spatial: Some(StaticM2Spatial::new(
            -Vec3::splat(10.),
            Vec3::splat(10.),
            15.,
            Mat4::from_translation(position),
        )),
        shadow_kind: Some(ModelShadowKind::StaticScenery),
        shadow_membership: 15,
        publishes_lights: false,
        doodad_active: false,
        doodad_visible: true,
        doodad_opacity: 1.,
    }
}

#[test]
fn static_collectors_keep_offscreen_shadows_lights_and_wmo_membership() -> Result<(), Box<dyn Error>>
{
    let view = view()?;
    let mut hidden = input(Vec3::ZERO);
    hidden.doodad_active = true;
    hidden.doodad_visible = false;
    hidden.doodad_opacity = 0.;
    let caster = hidden.evaluate(view)?;
    assert_ne!(
        caster.environment_maps, 0,
        "native collector precedes portal/opacity rejection"
    );
    assert!(!caster.rejected);
    hidden.shadow_membership = 0;
    assert!(
        hidden.evaluate(view)?.rejected,
        "an absent WMO collector membership has no caster"
    );
    hidden.publishes_lights = true;
    assert!(
        !hidden.evaluate(view)?.rejected,
        "hidden light owners still update"
    );

    let mut plain_view = view;
    plain_view.shadows = None;
    let far = input(Vec3::X * 1000.);
    assert!(far.evaluate(plain_view)?.rejected);
    let mut light = far;
    light.publishes_lights = true;
    assert!(!light.evaluate(plain_view)?.rejected);
    let fading = input(Vec3::X * 740.);
    let fade = fading.evaluate(view)?;
    assert_eq!(fade.scenery_opacity, 0.5);
    assert_eq!(
        fade.environment_maps, 0,
        "7BABC0 stops casters at fade start"
    );

    let narrow = WorldCamera::orthographic(
        Vec3::Z * 100.,
        Vec3::ZERO,
        Vec3::Y,
        [-1., 1.],
        [-1., 1.],
        0.1,
        200.,
    )
    .frame(1.)?;
    let mut narrow_view = view;
    narrow_view.frustum = WorldFrustum::new(narrow, WorldScreenWindow::FULL)?;
    let outside = input(Vec3::X * 80.);
    assert!(
        !narrow_view
            .frustum
            .contains_sphere(outside.spatial.ok_or("spatial")?.sphere().0, 15.)?
    );
    assert!(
        !outside.evaluate(narrow_view)?.rejected,
        "camera alone cannot remove a caster"
    );
    narrow_view.shadows = None;
    assert!(outside.evaluate(narrow_view)?.rejected);
    Ok(())
}

#[test]
fn static_groups_preserve_model_results_across_gaps_errors_and_abandonment()
-> Result<(), Box<dyn Error>> {
    let mut cpu = executor()?;
    let view = view()?;
    let mut batch = SpatialBatch::default();
    // More than one full group; odd indices stand for interleaved dynamic owners.
    for count in [137, 3, 0, 71] {
        batch.prepare(&cpu, count)?;
        for index in 0..count {
            batch.push(index * 2, input(Vec3::X * index as f32 * 7.), view)?;
        }
        batch.start()?;
        for index in 0..count {
            batch.wait_for(index * 2, &mut FrameWait::Offline)?;
            assert!(batch.is_ready(index * 2)?);
            let actual = batch.take(index * 2)?.ok_or("static result")?;
            assert_eq!(actual, input(Vec3::X * index as f32 * 7.).evaluate(view)?);
            assert!(batch.is_ready(index * 2 + 1)?);
            assert!(batch.take(index * 2 + 1)?.is_none());
        }
        batch.finish(&mut FrameWait::Offline)?;
    }

    batch.prepare(&cpu, 3)?;
    batch.push(0, input(Vec3::ZERO), view)?;
    let mut invalid = input(Vec3::ZERO);
    // NaN distance must not add a new error before the old collector's gate.
    invalid.spatial = Some(StaticM2Spatial::new(
        -Vec3::ONE,
        Vec3::ONE,
        1.,
        Mat4::from_translation(Vec3::NAN),
    ));
    batch.push(2, invalid, view)?;
    batch.push(4, input(Vec3::X * 50.), view)?;
    batch.start()?;
    batch.wait_for(0, &mut FrameWait::Offline)?;
    assert!(batch.is_ready(0)?);
    assert!(batch.take(0)?.is_some());
    assert!(batch.is_ready(2)?);
    assert_eq!(batch.take(2).is_err(), invalid.evaluate(view).is_err());
    // Drop the unconsumed suffix through explicit frame-abandonment reclamation.
    batch.finish(&mut FrameWait::Offline)?;
    batch.prepare(&cpu, 1)?;
    batch.push(0, input(Vec3::ZERO), view)?;
    batch.start()?;
    batch.wait_for(0, &mut FrameWait::Offline)?;
    assert!(batch.is_ready(0)?);
    assert_eq!(
        batch.take(0)?.ok_or("replacement result")?,
        input(Vec3::ZERO).evaluate(view)?
    );
    batch.finish(&mut FrameWait::Offline)?;
    drop(batch);
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn static_input_admission_refusal_leaves_the_phase_reusable() -> Result<(), Box<dyn Error>> {
    let mut cpu = executor()?;
    let mut batch = SpatialBatch::default();
    assert!(batch.prepare(&cpu, usize::MAX).is_err());
    batch.prepare(&cpu, 1)?;
    let view = view()?;
    batch.push(0, input(Vec3::ZERO), view)?;
    // Captured inputs may be abandoned before the admitted epoch publishes anything.
    batch.finish(&mut FrameWait::Offline)?;
    batch.prepare(&cpu, 1)?;
    batch.push(0, input(Vec3::ZERO), view)?;
    batch.start()?;
    batch.wait_for(0, &mut FrameWait::Offline)?;
    assert!(batch.is_ready(0)?);
    assert!(batch.take(0)?.is_some());
    batch.finish(&mut FrameWait::Offline)?;
    drop(batch);
    cpu.shutdown()?;
    Ok(())
}
