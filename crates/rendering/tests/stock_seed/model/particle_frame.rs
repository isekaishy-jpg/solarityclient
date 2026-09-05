//! Stock emitter-space composition through decoded models and live particles.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2ParticleMeshPlan, M2ParticlePose, M2ParticleSimulation,
    M2ParticleState, M2ParticleTwinkleTable, WorldCamera,
};

use super::{
    append_render_track, m2_array_offset, render_f32_values, render_m2_bytes, render_skin_bytes,
};
use crate::support::{Fixture, FixtureFile};

/// Stock `0x0097BE80` transforms centers but adds billboard offsets in view
/// space. Model-space storage must not introduce an extra card-size factor.
#[test]
fn particle_billboards_keep_view_space_size_under_attachment_transforms()
-> Result<(), Box<dyn Error>> {
    let mut ordinary = render_m2_bytes("Particle.blp", 1)?;
    let particle_offset = m2_array_offset(&ordinary, 0x128)?;
    ordinary[particle_offset + 4..particle_offset + 8]
        .copy_from_slice(&0x0002_0010_u32.to_le_bytes());
    let mut inherited = ordinary.clone();
    inherited[particle_offset + 4..particle_offset + 8]
        .copy_from_slice(&0x0002_0030_u32.to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\ViewSize.m2",
            bytes: &ordinary,
        },
        FixtureFile {
            path: "Creature\\Solarity\\ViewSize00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Creature\\Solarity\\InheritedViewSize.m2",
            bytes: &inherited,
        },
        FixtureFile {
            path: "Creature\\Solarity\\InheritedViewSize00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut assets = AssetStore::mount(catalog)?;
    let clock = M2AnimationClock::new(0, 500.0, 0.0);
    let particle = M2ParticleState::new(0.5, Vec3::new(0.25, -0.5, 0.75), Vec3::ZERO, 0x2483)?;
    let camera = WorldCamera::stock(Vec3::new(4.0, 3.0, 2.0), Vec3::ZERO, Vec3::Z, 100.0)
        .frame(16.0 / 9.0)?;
    let twinkle = M2ParticleTwinkleTable::new(0x0029_4823);
    for (name, inherits_size) in [("ViewSize", false), ("InheritedViewSize", true)] {
        let model = DecodedM2Model::load(
            &mut assets,
            &AssetPath::new(format!("Creature\\Solarity\\{name}.m2"))?,
        )?;
        let emitter = &model.animations().particles()[0];
        let pose = M2ParticlePose::sample(model.animations(), emitter, clock)?;
        let prepare = |transform: Mat4| {
            M2ParticleMeshPlan::prepare_transformed_with_particle_color(
                emitter,
                pose,
                &[particle],
                camera,
                transform,
                transform.x_axis.truncate().length(),
                1.0,
                &twinkle,
                None,
            )
        };
        let baseline = prepare(Mat4::IDENTITY)?;
        assert_eq!(baseline.vertices().len(), 4);
        for scale in [
            Vec3::splat(0.557),
            Vec3::splat(2.0),
            Vec3::new(0.5, 0.75, 1.25),
        ] {
            let transform = Mat4::from_translation(Vec3::new(2.0, -3.0, 1.0))
                * Mat4::from_rotation_z(0.75)
                * Mat4::from_scale(scale);
            let mesh = prepare(transform)?;
            let center = transform.transform_point3(particle.position());
            let size_factor = if inherits_size { scale.x } else { 1.0 };
            for (vertex, original) in mesh.vertices().iter().zip(baseline.vertices()) {
                let expected = center
                    + (Vec3::from_array(original.position()) - particle.position()) * size_factor;
                assert!(
                    (Vec3::from_array(vertex.position()) - expected)
                        .abs()
                        .max_element()
                        < 0.0001,
                    "{name}: scale={scale:?}, actual={:?}, expected={expected:?}",
                    vertex.position()
                );
            }
        }
    }
    Ok(())
}

/// `0x008309C0` applies the fixed +90-degree generator basis after translating
/// the emitter, before `0x00981950` creates its directed spherical particles.
/// The correction must affect bound/unbound emitters and both storage spaces.
#[test]
fn particle_generator_basis_preserves_origin_and_directs_live_births() -> Result<(), Box<dyn Error>>
{
    let world_bytes = directed_sphere(0x0002_0000, 2)?;
    let local_bytes = directed_sphere(0x0002_0010, 2)?;
    let unbound_bytes = directed_sphere(0x0002_2000, u16::MAX)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\WorldSphere.m2",
            bytes: &world_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\WorldSphere00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Creature\\Solarity\\LocalSphere.m2",
            bytes: &local_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\LocalSphere00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Creature\\Solarity\\UnboundSphere.m2",
            bytes: &unbound_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\UnboundSphere00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut assets = AssetStore::mount(catalog)?;
    // Exact nonuniform scale, +90-degree placement, and world translation.
    let placement = Mat4::from_cols(
        Vec4::new(0.0, 2.0, 0.0, 0.0),
        Vec4::new(-3.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 4.0, 0.0),
        Vec4::new(10.0, 20.0, 30.0, 1.0),
    );
    let clock = M2AnimationClock::new(0, 500.0, 0.0);
    for (name, expected_y) in [
        ("WorldSphere", 32.0),
        ("LocalSphere", 32.0),
        ("UnboundSphere", 28.0),
    ] {
        let model = DecodedM2Model::load(
            &mut assets,
            &AssetPath::new(format!("Creature\\Solarity\\{name}.m2"))?,
        )?;
        let emitter = model
            .animations()
            .particles()
            .first()
            .ok_or("missing sphere")?;
        let bones = M2BonePose::compose(model.animations(), clock)?;
        let transform = bones.particle_emitter_transform(emitter, placement)?;
        // Bone 2 inherits the root's +2 X translation at 500 ms. The unbound
        // declaration omits that translation, but keeps the generator remap.
        assert_eq!(
            transform.transform_point3(Vec3::ZERO),
            Vec3::new(16.0, expected_y, 34.0)
        );
        assert_eq!(
            transform.transform_vector3(Vec3::X),
            Vec3::new(-3.0, 0.0, 0.0)
        );
        assert_eq!(
            transform.transform_vector3(Vec3::Y),
            Vec3::new(0.0, -2.0, 0.0)
        );
        assert_eq!(
            transform.transform_vector3(Vec3::Z),
            Vec3::new(0.0, 0.0, 4.0)
        );

        let pose = M2ParticlePose::sample(model.animations(), emitter, clock)?;
        let mut simulation = M2ParticleSimulation::new(0);
        let report = simulation.advance_sphere(emitter, pose, 0.1, transform, 1.0)?;
        assert_eq!(report.live(), 1);
        let particle = simulation.particles().first().ok_or("missing live birth")?;
        let (position, velocity) = if emitter.particles_in_model_space() {
            (
                transform.transform_point3(particle.position()),
                transform.transform_vector3(particle.velocity()),
            )
        } else {
            (particle.position(), particle.velocity())
        };
        // A unit-radius +X birth with speed 2 now launches along world -X.
        assert!(
            (position - Vec3::new(12.4, expected_y, 34.0))
                .abs()
                .max_element()
                < 0.0001,
            "{name}: {position:?}"
        );
        assert_eq!(velocity, Vec3::new(-6.0, 0.0, 0.0));
    }
    Ok(())
}

/// `0x00981950` computes launch direction before multiplying by shell radius.
/// The zero-radius Bloodmage case must retain its authored emission speed.
#[test]
fn sphere_launch_direction_is_independent_of_zero_or_negative_radius() -> Result<(), Box<dyn Error>>
{
    for radius in [0.0, -1.0] {
        let mut bytes = directed_sphere(0x0002_0010, 2)?;
        let offset = m2_array_offset(&bytes, 0x128)?;
        for field in [0x0c8, 0x0dc] {
            append_render_track(
                &mut bytes,
                offset + field,
                &[0],
                &render_f32_values(&[radius]),
                4,
            )?;
        }
        let skin = render_skin_bytes()?;
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "Creature\\Solarity\\RadiusSphere.m2",
                bytes: &bytes,
            },
            FixtureFile {
                path: "Creature\\Solarity\\RadiusSphere00.skin",
                bytes: &skin,
            },
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut assets = AssetStore::mount(catalog)?;
        let model = DecodedM2Model::load(
            &mut assets,
            &AssetPath::new("Creature\\Solarity\\RadiusSphere.m2")?,
        )?;
        let emitter = &model.animations().particles()[0];
        let pose = M2ParticlePose::sample(
            model.animations(),
            emitter,
            M2AnimationClock::new(0, 500.0, 0.0),
        )?;
        let mut simulation = M2ParticleSimulation::new(0);
        let report = simulation.advance_sphere(emitter, pose, 0.1, Mat4::IDENTITY, 1.0)?;
        assert_eq!(report.live(), 1);
        let particle = &simulation.particles()[0];
        assert_eq!(particle.velocity(), Vec3::X * 2.0, "radius={radius}");
        assert!(
            (particle.position() - Vec3::X * (radius + 0.2))
                .abs()
                .max_element()
                < 0.0001
        );
    }
    Ok(())
}

/// Native spherical z-source aim retains subthreshold vectors without normalization.
#[test]
fn sphere_z_source_aim_uses_stock_squared_length_threshold() -> Result<(), Box<dyn Error>> {
    for (z_source, speed) in [(0.0001, 0.0002), (0.001, 2.0)] {
        let mut bytes = directed_sphere(0x0002_0010, 2)?;
        let offset = m2_array_offset(&bytes, 0x128)?;
        for (field, value) in [(0x0c8, 0.0), (0x0dc, 0.0), (0x0f0, z_source)] {
            append_render_track(
                &mut bytes,
                offset + field,
                &[0],
                &render_f32_values(&[value]),
                4,
            )?;
        }
        let skin = render_skin_bytes()?;
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "Creature\\Solarity\\AimSphere.m2",
                bytes: &bytes,
            },
            FixtureFile {
                path: "Creature\\Solarity\\AimSphere00.skin",
                bytes: &skin,
            },
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut assets = AssetStore::mount(catalog)?;
        let model = DecodedM2Model::load(
            &mut assets,
            &AssetPath::new("Creature\\Solarity\\AimSphere.m2")?,
        )?;
        let emitter = &model.animations().particles()[0];
        let pose = M2ParticlePose::sample(
            model.animations(),
            emitter,
            M2AnimationClock::new(0, 500.0, 0.0),
        )?;
        let mut simulation = M2ParticleSimulation::new(0);
        simulation.advance_sphere(emitter, pose, 0.1, Mat4::IDENTITY, 1.0)?;
        assert!(
            (simulation.particles()[0].velocity() - Vec3::NEG_Z * speed)
                .abs()
                .max_element()
                < 0.000001
        );
    }
    Ok(())
}

/// Isolates the narrow spherical launch used by the foreground Night Elf dust.
fn directed_sphere(flags: u32, bone: u16) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Sphere", 1)?;
    let offset = m2_array_offset(&bytes, 0x128)?;
    bytes[offset + 4..offset + 8].copy_from_slice(&flags.to_le_bytes());
    bytes[offset + 8..offset + 20].copy_from_slice(&render_f32_values(&[4.0, -2.0, 1.0]));
    bytes[offset + 20..offset + 22].copy_from_slice(&bone.to_le_bytes());
    bytes[offset + 41] = 2;
    for (field, value) in [
        (0x034, 2.0),
        (0x048, 0.0),
        (0x05c, 0.0),
        (0x070, 0.0),
        (0x084, 0.0),
        (0x098, 4.0),
        (0x0b0, 10.0),
        (0x0c8, 1.0),
        (0x0dc, 1.0),
        (0x0f0, 0.0),
    ] {
        append_render_track(
            &mut bytes,
            offset + field,
            &[0],
            &render_f32_values(&[value]),
            4,
        )?;
    }
    append_render_track(&mut bytes, offset + 0x1c8, &[0], &[1], 1)?;
    Ok(bytes)
}
