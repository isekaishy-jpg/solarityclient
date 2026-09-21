//! CPU admission preserves stock effect arithmetic and bounds retained allocations.

use super::*;
use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStoragePlan};

/// Identical authored inputs and seeds produce identical simulation despite
/// physical reservation; the stock live limit is still chosen by each update.
#[test]
fn admitted_effects_preserve_simulation_and_own_their_capacity() -> Result<(), Box<dyn Error>> {
    let bytes = render_m2_bytes("Effects.blp", 1)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\Effects.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Effects00.skin",
            bytes: &skin,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Creature\\Solarity\\Effects.m2")?,
    )?;
    let emitter = &model.animations().particles()[0];
    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20));
    let mut ordinary = M2ParticleSimulation::new(47);
    let mut admitted = M2ParticleSimulation::new(47);
    admitted.reserve_cpu_storage(&budget, M2ParticleSimulation::authored_capacity(emitter)?)?;
    assert_eq!(admitted.capacity(), ordinary.capacity());
    let particle_bytes = admitted.allocated_bytes();
    for index in 0..128 {
        let delta = [0.07, 0.03, 0.1, 0.2, 0.0][index % 5];
        let transform = Mat4::from_translation(Vec3::new(index as f32 * 0.02, 0., 0.));
        assert_eq!(
            ordinary.advance_planar_bounded(emitter, pose, delta, transform, 1.0)?,
            admitted.advance_planar_bounded(emitter, pose, delta, transform, 1.0)?
        );
        assert_eq!(ordinary.particles(), admitted.particles());
        assert_eq!(ordinary.capacity(), admitted.capacity());
        assert_eq!(ordinary.emission_remainder(), admitted.emission_remainder());
        assert_eq!(admitted.allocated_bytes(), particle_bytes);
    }
    let retained = admitted.particles().to_vec();
    assert!(admitted.reserve_cpu_storage(&budget, 1 << 20).is_err());
    assert_eq!(admitted.particles(), retained);
    assert_eq!(admitted.allocated_bytes(), particle_bytes);

    let ribbon = &model.animations().ribbons()[0];
    let pose = M2RibbonPose::sample(
        model.animations(),
        ribbon,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    let mut trail = M2RibbonTrail::new(ribbon)?;
    trail.reserve_cpu_storage(&budget)?;
    let ribbon_bytes = trail.allocated_bytes();
    for index in 0..128 {
        let point = M2RibbonControlPoint::new(Vec3::X * index as f32, Vec3::Y, Vec3::X);
        trail.advance(if index % 2 == 0 { 0.01 } else { 4.0 }, point, pose)?;
        assert!(trail.sections().len() <= trail.capacity());
        assert_eq!(trail.allocated_bytes(), ribbon_bytes);
    }
    assert_eq!(
        budget.snapshot().used(CpuStorageClass::Frame),
        particle_bytes + ribbon_bytes
    );
    admitted.reset();
    trail.reset();
    assert_eq!(
        budget.snapshot().used(CpuStorageClass::Frame),
        particle_bytes + ribbon_bytes
    );
    drop(admitted);
    drop(trail);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}
