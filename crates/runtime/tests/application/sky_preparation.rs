//! Retained weather simulation crosses workers without resetting time or noise.
use super::*;
use crate::application::frame_pipeline::FrameWait;
use crate::test_support::ClientFixture;
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, LightCatalog, Locale};

#[test]
fn sky_update_overlaps_admission_and_restores_abandoned_frames()
-> Result<(), Box<dyn std::error::Error>> {
    let colors = (1..=18)
        .flat_map(|id| band(id, 0x8090a0))
        .collect::<Vec<_>>();
    let floats = (1..=6)
        .flat_map(|id| band(id, if id == 1 { 18000_f32 } else { 0.5 }.to_bits()))
        .collect::<Vec<_>>();
    let fixture = ClientFixture::with_common_files(&[
        (
            "DBFilesClient/Light.dbc",
            &table(15, &[1, 571, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]),
        ),
        (
            "DBFilesClient/LightParams.dbc",
            &table(9, &[1, 0, 0, 0, 0, 0, 0, 0, 0]),
        ),
        ("DBFilesClient/LightIntBand.dbc", &table(34, &colors)),
        ("DBFilesClient/LightFloatBand.dbc", &table(34, &floats)),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let mut environment =
        crate::RuntimeWorldEnvironment::new(LightCatalog::load(&mut store)?, 8 << 30)?;
    let world = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(571),
        1,
        "Sky",
        Vec3::ZERO,
        0.,
    ));
    let clock = crate::RealmClock::new(solarity_network::WorldTimeSpeed::new(0, 0., 0)?);
    let environment = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("environment")?;
    let cpu = crate::frame_cpu_support::executor()?;
    let mut owner = WorldSkyPreparation::new();
    let mut serial = WorldSky::new();
    for (index, time) in [0, 16, 250, 1250].into_iter().enumerate() {
        let eye = Vec3::new(50. * index as f32, -20., index as f32);
        let camera =
            solarity_rendering::WorldCamera::stock(eye, eye + Vec3::X, Vec3::Z, 1000.).frame(1.)?;
        let colors = [0xff112233 + index as u32, 0xff445566, 0xff778899];
        serial.update(environment, camera, time, colors);
        let held = crate::frame_cpu_support::continuation_support::HeldFrameWorkers::new(&cpu)?;
        let mut update = owner.start(&cpu, environment, camera, time, colors)?;
        let pending = !update.owner.batch.is_finished();
        held.release()?;
        assert!(
            pending,
            "main must proceed while sky computation is pending"
        );
        if index % 2 == 0 {
            update.finish(&mut FrameWait::Offline)?;
        }
        // Odd frames deliberately abandon presentation before the result consumer.
        drop(update);
        assert_eq!(owner.jobs.len(), 1);
        let actual = &owner.jobs[0].sky;
        assert_eq!(actual.gradient.colors(), serial.gradient.colors());
        assert_eq!(actual.celestial_meshes, serial.celestial_meshes);
        assert_eq!(
            actual.cloud_frame(camera).bgra8(),
            serial.cloud_frame(camera).bgra8()
        );
        assert_eq!(actual.elapsed_seconds, serial.elapsed_seconds);
        assert_eq!(actual.last_update_ms, Some(time));
        for body in actual.celestials.bodies() {
            assert_eq!(
                actual.clouds.opacity_at(eye, body.position()),
                serial.clouds.opacity_at(eye, body.position())
            );
        }
    }
    Ok(())
}

fn band(id: u32, value: u32) -> [u32; 34] {
    let mut row = [0; 34];
    row[0] = id;
    row[1] = 1;
    row[18] = value;
    row
}
fn table(width: usize, words: &[u32]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [
        words.len() as u32 / width as u32,
        width as u32,
        width as u32 * 4,
        1,
    ]
    .iter()
    .chain(words)
    {
        bytes.extend(word.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
