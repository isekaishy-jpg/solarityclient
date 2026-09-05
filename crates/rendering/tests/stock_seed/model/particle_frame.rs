//! Stock emitter-space composition through decoded models and live particles.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{M2AnimationClock, M2BonePose, M2ParticlePose, M2ParticleSimulation};

use super::{
    append_render_track, m2_array_offset, render_f32_values, render_m2_bytes, render_skin_bytes,
};
use crate::support::{Fixture, FixtureFile};

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
