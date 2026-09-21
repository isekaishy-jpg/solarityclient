//! Installed terrain must finish GPU publication before the loading card closes.

use super::*;
use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, PlayerAppearance, PlayerEquipment,
    UnitAnimationTier, UnitFlags, UnitIdentity, UnitPresentation, UnitSheathState, WorldBootstrap,
    WorldMapId,
};
use std::{
    error::Error,
    ffi::OsString,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn installed_world_entry_waits_for_camera_terrain_and_detail() -> Result<(), Box<dyn Error>> {
    let _guard = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock poisoned")?;
    let data = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let profile = crate::test_support::ClientFixture::new()?;
    let mut arguments: Vec<OsString> = [
        "--locale",
        "enUS",
        "--cpu-workers",
        "4",
        "--cpu-capacity",
        "256",
        "--network-workers",
        "1",
        "--network-shutdown-ms",
        "250",
        "--login-endpoint",
        "127.0.0.1:3724",
        "--login-timezone-minutes",
        "-240",
        "--login-client-ip",
        "127.0.0.1",
        "--window-width",
        "320",
        "--window-height",
        "240",
        "--window-mode",
        "windowed",
        "--gpu-index",
        "0",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    arguments.extend([
        OsString::from("--data-root"),
        data,
        OsString::from("--profile-root"),
        profile.profile_root().as_os_str().to_owned(),
    ]);
    let configuration = RuntimeConfiguration::from_arguments(arguments)?;
    let (mut services, _, _) = ClientServices::start(
        &configuration,
        crate::application::client_services::StartupVisibility::Hidden,
    )?;
    assert!(!services.service_terrain_streaming()?);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(1),
        1,
        "Entry",
        Vec3::new(1299.02, -4373.88, 40.),
        0.,
    ));
    let player = world.local_player();
    world.storage_mut().add_component(
        player,
        (
            ObjectKind::Player,
            ObjectPresentation::new(0, 1.),
            UnitIdentity::new(1, 1, 0, 1, 1, 1),
            UnitPresentation::new(
                49,
                49,
                0,
                0,
                UnitAnimationTier::Ground,
                UnitSheathState::Unarmed,
            ),
            UnitFlags::default(),
            PlayerAppearance::default(),
            PlayerEquipment::default(),
        ),
    );
    services.terrain.synchronize(Some(&world))?;
    services.player.synchronize(Some(&world))?;
    let clock = crate::RealmClock::new(solarity_network::WorldTimeSpeed::new(12 << 6, 0., 0)?);
    services
        .environment
        .synchronize(Some(&world), Some(&clock))?;
    let resident = services
        .terrain
        .resident_tiles()
        .next()
        .ok_or("primary tile")?;
    let (filtering, mip) = services.renderer.file_texture_sampling();
    services.terrain_frame = Some(TerrainFrame::prepare(
        &mut services.renderer,
        1,
        resident.mesh(),
        resident.textures(),
        resident.liquid_batches(),
        resident.m2_scene(),
        resident.world_models(),
        filtering,
        mip,
        &mut services.crt_rand,
        Arc::clone(&services.particle_twinkle),
        services.player.resident_frame_input(),
        &[],
        &[],
        services.game_objects.frame_input(Some(&world)),
    )?);
    services.loading_screen = Some(RuntimeLoadingScreen::prepare(
        &mut services.renderer,
        &services.assets,
        &mut services.ui_textures,
        &services.loading_directory,
        Some(1),
        (320, 240),
    )?);
    assert_eq!(services.terrain.resident_tile_count(), 1);
    assert!(
        !services.service_terrain_streaming()?,
        "primary terrain cannot complete entry"
    );
    let deadline = Instant::now() + Duration::from_secs(180);
    let started = Instant::now();
    loop {
        let terrain_ready = services.service_terrain_streaming()?;
        let detail_ready = services.prepare_world_entry_detail()?;
        let loading = services.loading_screen.as_mut().ok_or("loading card")?;
        loading.advance(RuntimeLoadingReadiness {
            world_accepted: true,
            environment_ready: true,
            player_ready: true,
            scene_ready: terrain_ready && detail_ready,
            ui_ready: true,
            transport_resource_ready: true,
        });
        if !terrain_ready || !detail_ready {
            assert!(!loading.ready_to_complete());
        }
        loading.present(&mut services.renderer, &[], &mut |pending| pending.wait())?;
        if loading.ready_to_complete() {
            assert!(terrain_ready && detail_ready);
            assert_eq!(services.terrain.resident_tile_count(), 49);
            break;
        }
        assert!(Instant::now() < deadline, "entry terrain did not finish");
        for _ in 0..256 {
            if services.poll_platform_event().is_none() {
                break;
            }
        }
        std::thread::yield_now();
    }
    println!(
        "covered terrain/detail ready after {:.3}s",
        started.elapsed().as_secs_f64()
    );
    let loading = services.loading_screen.as_mut().ok_or("loading card")?;
    loading.advance(RuntimeLoadingReadiness::default());
    assert!(
        !loading.ready_to_complete(),
        "old completed progress cannot override current readiness"
    );
    services.terrain.disconnect();
    assert!(
        !services.service_terrain_streaming()?,
        "a retired map cannot inherit readiness"
    );
    services.shutdown()?;
    Ok(())
}
