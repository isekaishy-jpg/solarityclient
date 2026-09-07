//! Encrypted movement dispatch through the resident terrain and publication phase.

use std::{collections::VecDeque, sync::Arc};

use glam::Vec3;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog,
    CharacterRaceCatalog, CharacterStartOutfitCatalog, ClientDataRoot, CreatureCatalog,
    CreatureFamilyCatalog, GameObjectDisplayCatalog, HelmetGeosetVisibilityCatalog,
    ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog, Locale, MapCatalog,
    ParticleColorCatalog,
};
use solarity_ecs::{
    ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldMovementContext, WorldMovementSpeeds,
    WorldMovementState, WorldTransform,
};

use super::{RuntimeGameplayCoordinator, RuntimePlayerControl, dispatch_world_packet};
use crate::application::player_movement::remote::RuntimeRemoteMovement;
use crate::application::{
    RuntimeGameObjectPresentation, RuntimePlayerCatalogs, RuntimePlayerItemCatalogs,
    RuntimePlayerPresentation, RuntimeTerrainCoordinator,
};
use crate::test_network::{TestError, WorldServer};

#[test]
fn encrypted_remote_walk_run_stop_reaches_ecs_without_moving_active_or_unknown_units()
-> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let fixture =
                super::movement_entry_tests::flat_world().map_err(|error| error.to_string())?;
            let mut store = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let maps = MapCatalog::load(&mut store)?;
            let displays = GameObjectDisplayCatalog::load(&mut store)?;
            let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
            let catalogs = RuntimePlayerCatalogs::new(
                animations.clone(),
                CreatureCatalog::load(&mut store)?,
                CreatureFamilyCatalog::default(),
                CharacterAppearanceCatalog::load(&mut store)?,
                CharacterRaceCatalog::load(&mut store)?,
                HelmetGeosetVisibilityCatalog::load(&mut store)?,
                CharacterStartOutfitCatalog::load(&mut store)?,
                RuntimePlayerItemCatalogs::new(
                    ItemDefinitionCatalog::load(&mut store)?,
                    ItemDisplayCatalog::load(&mut store)?,
                    ItemVisualCatalog::load(&mut store)?,
                ),
                ParticleColorCatalog::load(&mut store)?,
            );
            let assets = AssetStoreHandle::new(store);
            let mut terrain = RuntimeTerrainCoordinator::new(assets.clone(), maps);
            let objects = RuntimeGameObjectPresentation::new(assets.clone(), displays, animations);
            let presentation = RuntimePlayerPresentation::new(assets, catalogs);
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(571),
                1,
                "Observer",
                Vec3::new(1000.0, 5800.0, 10.0),
                0.0,
            ));
            let speeds = WorldMovementSpeeds::new([
                2.5,
                7.0,
                4.5,
                4.72,
                2.5,
                7.0,
                4.5,
                std::f32::consts::PI,
                std::f32::consts::PI,
            ]);
            for (guid, kind) in [(9, ObjectKind::Unit), (10, ObjectKind::Player)] {
                world.create_object(
                    guid,
                    kind,
                    Some(WorldTransform::new(Vec3::new(1000.0, 5800.0, 10.0), 0.0)),
                    [],
                )?;
                world.update_movement(
                    guid,
                    WorldMovementState::new(0, speeds, WorldMovementContext::default()),
                )?;
            }
            let identity = world.object_identity(1).ok_or("active identity")?;
            let mut control = RuntimePlayerControl::new(identity);
            let mut gameplay = RuntimeGameplayCoordinator::with_test_world(world);
            let mut movement = RuntimeRemoteMovement::default();
            terrain.synchronize(gameplay.world())?;
            terrain.synchronize_game_object_movement(
                gameplay.world(),
                &objects,
                solarity_systems::MovementBspCacheMode::Enabled,
            )?;
            let (server, mut network) = WorldServer::connect().await?;
            let sent = server
                .exchange(
                    vec![
                        (0xb5, ordinary(9, 0x101, 0, 1000.0)),
                        (0xb5, ordinary(10, 1, 0, 1000.0)),
                        (0xb5, ordinary(1, 1, 0, 9000.0)),
                        (0xb5, ordinary(77, 1, 0, 9000.0)),
                    ],
                    0,
                )
                .await?;
            let mut unhandled = VecDeque::new();
            for expected in [true, true, false, false] {
                assert_eq!(
                    dispatch_world_packet(
                        gameplay.world.as_mut().ok_or("world")?,
                        network.receive_packet().await?,
                        &mut None,
                        &mut None,
                        &mut control,
                        &mut unhandled,
                        1.0,
                        &mut |_, _, _| Ok(()),
                        0
                    )?,
                    expected
                );
            }
            sent.await??;
            movement.service(&gameplay, &mut terrain, &objects, &presentation, 0)?;
            for now in [250, 500, 750, 1000] {
                movement.service(&gameplay, &mut terrain, &objects, &presentation, now)?;
            }
            let world = gameplay.world().ok_or("world")?;
            for (guid, expected_x) in [(9, 1002.5_f32), (10, 1007.0)] {
                let position = world
                    .object_transform(guid)
                    .ok_or("remote transform")?
                    .position();
                assert!(
                    (position.x - expected_x).abs() < 0.001,
                    "{guid}: {position:?}"
                );
                assert!((position.z - 10.0).abs() < 0.01);
            }
            assert_eq!(world.local_player_transform()?.position().x, 1000.0);
            assert!(world.entity_by_guid(77).is_none());
            assert!(unhandled.is_empty());
            let sent = server
                .exchange(vec![(0xb7, ordinary(9, 0, 1000, 1002.5))], 0)
                .await?;
            dispatch_world_packet(
                gameplay.world.as_mut().ok_or("world")?,
                network.receive_packet().await?,
                &mut None,
                &mut None,
                &mut control,
                &mut unhandled,
                1.0,
                &mut |_, _, _| Ok(()),
                1000,
            )?;
            sent.await??;
            movement.service(&gameplay, &mut terrain, &objects, &presentation, 1250)?;
            assert_eq!(
                gameplay
                    .world()
                    .ok_or("world")?
                    .object_transform(9)
                    .ok_or("stopped unit")?
                    .position()
                    .x,
                1002.5
            );
            // Reusing the GUID has a new identity and cannot retain the previous
            // lifetime's clock, pending commands, or collision trajectory.
            let world = gameplay.world.as_mut().ok_or("world")?;
            world.remove_object(10)?;
            world.create_object(
                10,
                ObjectKind::Player,
                Some(WorldTransform::new(Vec3::new(1000.0, 5800.0, 10.0), 0.0)),
                [],
            )?;
            world.update_movement(
                10,
                WorldMovementState::new(0, speeds, WorldMovementContext::default()),
            )?;
            movement.service(&gameplay, &mut terrain, &objects, &presentation, 1500)?;
            assert_eq!(
                gameplay
                    .world()
                    .ok_or("world")?
                    .object_transform(10)
                    .ok_or("replacement")?
                    .position()
                    .x,
                1000.0
            );
            let sent = server
                .exchange(
                    vec![
                        (0xb5, ordinary(9, 0x101, 1500, 1002.5)),
                        (0xb7, ordinary(9, 0, 2500, 1005.0)),
                        (0xdd, path(9, 1002.5, 1010.0)),
                    ],
                    0,
                )
                .await?;
            for _ in 0..3 {
                assert!(dispatch_world_packet(
                    gameplay.world.as_mut().ok_or("world")?,
                    network.receive_packet().await?,
                    &mut None,
                    &mut None,
                    &mut control,
                    &mut unhandled,
                    1.0,
                    &mut |_, _, _| Ok(()),
                    1500
                )?);
            }
            sent.await??;
            movement.service(&gameplay, &mut terrain, &objects, &presentation, 1500)?;
            assert_eq!(
                gameplay
                    .world()
                    .ok_or("world")?
                    .object_transform(9)
                    .ok_or("path start")?
                    .position()
                    .x,
                1005.0
            );
            movement.service(&gameplay, &mut terrain, &objects, &presentation, 1750)?;
            let position = gameplay
                .world()
                .ok_or("world")?
                .object_transform(9)
                .ok_or("path sample")?
                .position();
            assert!((position.x - 1005.25).abs() < 0.001, "{position:?}");
            assert!(
                gameplay
                    .world()
                    .ok_or("world")?
                    .movement_state(9)
                    .ok_or("path movement")?
                    .spline()
                    .is_some()
            );
            Ok(())
        })
}

/// One full linear destination with no packed intermediate offsets.
fn path(guid: u8, start_x: f32, end_x: f32) -> Vec<u8> {
    let mut body = vec![1, guid, 0];
    for value in [start_x, 5800.0, 10.0] {
        body.extend(value.to_le_bytes());
    }
    body.extend(17_u32.to_le_bytes());
    body.push(0);
    for word in [0_u32, 5000, 1] {
        body.extend(word.to_le_bytes());
    }
    for value in [end_x, 5800.0, 10.0] {
        body.extend(value.to_le_bytes());
    }
    body
}

/// Exact packed GUID and MovementInfo used by the native ordinary opcode family.
fn ordinary(guid: u8, flags: u32, time_ms: u32, x: f32) -> Vec<u8> {
    let mut body = vec![1, guid];
    body.extend(flags.to_le_bytes());
    body.extend(0_u16.to_le_bytes());
    body.extend(time_ms.to_le_bytes());
    for value in [x, 5800.0, 10.0, 0.0] {
        body.extend(value.to_le_bytes());
    }
    body.extend(0_u32.to_le_bytes());
    body
}
