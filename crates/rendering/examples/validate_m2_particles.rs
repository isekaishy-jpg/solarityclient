//! Advances installed M2 particles and checks their geometry against camera zero.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2CameraEffectScale, M2ParticleMeshPlan, M2ParticlePose,
    M2ParticleSimulation, M2ParticleTwinkleTable, sample_m2_camera_frame,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or_else(usage_error)?);
    let locale = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    let path = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?;
    if args.next().is_some() {
        return Err(usage_error().into());
    }
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, locale)?;
    let mut assets = AssetStore::mount(catalog)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new(path)?)?;
    let mut simulations = model
        .animations()
        .particles()
        .iter()
        .enumerate()
        .map(|(index, _)| M2ParticleSimulation::new(index as u32))
        .collect::<Vec<_>>();
    let twinkle = M2ParticleTwinkleTable::new(0);
    // This diagnostic fixes the timestep and seeds for repeatable CPU coverage;
    // it does not reproduce the live client's process-wide random history.
    for tick in 1..=1_200 {
        let time_ms = (tick as f32 * 1_000.0) / 60.0;
        let clock = M2AnimationClock::new(0, time_ms, time_ms);
        let camera = sample_m2_camera_frame(model.animations(), 0, clock, 16.0 / 9.0)?;
        let bones = M2BonePose::compose_with_model_view(model.animations(), clock, camera.view())?;
        for (index, (emitter, simulation)) in model
            .animations()
            .particles()
            .iter()
            .zip(&mut simulations)
            .enumerate()
        {
            let pose = M2ParticlePose::sample(model.animations(), emitter, clock)?;
            let transform = bones.particle_emitter_transform(emitter, Mat4::IDENTITY)?;
            let density = if emitter.flags() & 0x0040_0000 != 0 {
                1.0
            } else {
                (1.0 - (camera.camera().position().length() - 50.0) * 0.02).clamp(0.25, 1.0)
            };
            match emitter.emitter_type() {
                1 => simulation.advance_planar_bounded(
                    emitter,
                    pose,
                    1.0 / 60.0,
                    transform,
                    density,
                )?,
                2 => simulation.advance_sphere_bounded(
                    emitter,
                    pose,
                    1.0 / 60.0,
                    transform,
                    density,
                )?,
                value => return Err(format!("unsupported emitter type {value} at {index}").into()),
            };
            if tick % 300 != 0 {
                continue;
            }
            let particle_to_world = if emitter.particles_in_model_space() {
                transform
            } else {
                Mat4::IDENTITY
            };
            let inherited_scale = transform.x_axis.truncate().length()
                * M2CameraEffectScale::from_native_camera(&camera).factor();
            let mesh = M2ParticleMeshPlan::prepare_transformed_with_twinkle_table(
                emitter,
                pose,
                simulation.particles(),
                camera,
                particle_to_world,
                inherited_scale,
                1.0,
                &twinkle,
            )?;
            let mut bounds_min = Vec3::splat(f32::INFINITY);
            let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
            let mut in_frustum = 0;
            let mut nonzero_alpha = 0;
            let mut clip_codes = Vec::with_capacity(mesh.vertices().len());
            for vertex in mesh.vertices() {
                let position = Vec3::from_array(vertex.position());
                if !position.is_finite() {
                    return Err(format!("non-finite emitter {index} vertex").into());
                }
                bounds_min = bounds_min.min(position);
                bounds_max = bounds_max.max(position);
                let clip = camera.view_projection() * position.extend(1.0);
                clip_codes.push(clip_outcode(clip));
                if clip.w > 0.0
                    && clip.x.abs() <= clip.w
                    && clip.y.abs() <= clip.w
                    && clip.z >= 0.0
                    && clip.z <= clip.w
                {
                    in_frustum += 1;
                }
                nonzero_alpha += usize::from(vertex.color_bgra()[3] != 0);
            }
            // A shared outside plane rejects the whole triangle. Candidates
            // can still be clipped or occluded; zero candidates proves that
            // no triangle can reach rasterization, even for oversized cards.
            let (triangles, trailing) = mesh.indices().as_chunks::<3>();
            debug_assert!(trailing.is_empty());
            let candidate_triangles = triangles
                .iter()
                .filter(|triangle| {
                    triangle
                        .iter()
                        .fold(0x3f, |shared, index| shared & clip_codes[*index as usize])
                        == 0
                })
                .count();
            println!(
                "time_ms={time_ms} emitter={index} live={} vertices={} in_frustum={in_frustum} candidate_triangles={candidate_triangles} nonzero_alpha={nonzero_alpha} bounds={bounds_min:?}..{bounds_max:?}",
                simulation.particles().len(),
                mesh.vertices().len()
            );
        }
    }
    Ok(())
}

/// Six homogeneous Vulkan clip planes; every bit identifies an outside half-space.
fn clip_outcode(point: Vec4) -> u8 {
    u8::from(point.x < -point.w)
        | (u8::from(point.x > point.w) << 1)
        | (u8::from(point.y < -point.w) << 2)
        | (u8::from(point.y > point.w) << 3)
        | (u8::from(point.z < 0.0) << 4)
        | (u8::from(point.z > point.w) << 5)
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_m2_particles <Data directory> <locale> <M2 path>",
    )
}
