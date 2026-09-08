//! Native life admission and packet-mirror callback ordering.

use crate::application::gameplay_coordinator::player_ui::{
    RuntimePlayerLifeEvent, RuntimePlayerUiNotification, RuntimePlayerUiState,
};
use crate::application::gameplay_session::{
    GameplayUpdateError, UnitFieldNotification, apply_object_updates_with_units,
};
use crate::test_network::{TestError, WorldServer};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};

fn world(health: u32) -> Result<ActiveWorld, TestError> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        glam::Vec3::ZERO,
        0.,
    ));
    let fields = [(24, health), (32, 100), (1197, 8)];
    world.create_object(7, ObjectKind::Player, None, fields)?;
    solarity_systems::project_object_fields(&mut world, 7, fields)?;
    Ok(world)
}

fn events(state: &mut RuntimePlayerUiState) -> Vec<&'static str> {
    let mut events = Vec::new();
    while let Some(notification) = state.take_notification() {
        match notification {
            RuntimePlayerUiNotification::Life { event, .. } => match event {
                RuntimePlayerLifeEvent::Dead => events.push("dead"),
                RuntimePlayerLifeEvent::Alive => events.push("alive"),
                RuntimePlayerLifeEvent::Flags { unghost } => {
                    events.push("flags");
                    if unghost {
                        events.push("unghost");
                    }
                }
            },
            RuntimePlayerUiNotification::Health {
                health_changed,
                maximum_changed,
                ..
            } => {
                if health_changed {
                    events.push("health");
                }
                if maximum_changed {
                    events.push("maximum");
                }
            }
            _ => panic!("unexpected life fixture notification"),
        }
    }
    events
}

#[test]
fn player_life_admission_matches_native_signed_health_and_notice() -> Result<(), TestError> {
    for line in include_str!("../fixtures/player_life_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_ascii_whitespace().collect();
        let old = u32::from_str_radix(row[1], 16)?;
        if row[0] == "flags" {
            let mut world = world(100)?;
            world.update_fields(7, [(150, u32::from_str_radix(row[2], 16)?)])?;
            let mut state = RuntimePlayerUiState::default();
            state.receive_unit_field(
                &world,
                world.object_identity(7).ok_or("identity")?,
                UnitFieldNotification::PlayerFlags { previous: old },
                1000,
            );
            let expected = row[3]
                .split(',')
                .map(|event| match event {
                    "18f" => "flags",
                    "188" => "unghost",
                    _ => panic!("unknown native ghost event"),
                })
                .collect::<Vec<_>>();
            assert_eq!(events(&mut state), expected, "{line}");
            continue;
        }
        let health = u32::from_str_radix(row[2], 16)?;
        let world = world(health)?;
        let mut state = RuntimePlayerUiState::default();
        state.receive_unit_field(
            &world,
            world.object_identity(7).ok_or("local identity")?,
            UnitFieldNotification::Health { previous: old },
            1000,
        );
        let expected = if row[4] == "none" {
            vec!["health"]
        } else {
            vec![row[4], "health"]
        };
        assert_eq!(events(&mut state), expected, "{line}");
        if row[4] == "dead" {
            assert_eq!(state.release_timer().remaining(1000), 360);
        }
        state.receive_death_notice(&world, 2000);
        assert_eq!(
            events(&mut state),
            [if row[5] == "102" { "dead" } else { "alive" }]
        );
        assert_eq!(state.release_timer().remaining(2000), 360);
    }
    Ok(())
}

#[test]
fn player_life_callbacks_read_last_packet_mirror_and_final_raw_image() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut session) = WorldServer::connect().await?;
            for (initial, blocks, expected) in [
                (100, vec![vec![(24, 0)]], vec!["dead", "health"]),
                (
                    100,
                    vec![vec![(24, 0)], vec![(24, 100)]],
                    vec!["alive", "health", "alive", "health"],
                ),
                // Every watched range was mirrored by the later maximum-only block.
                (100, vec![vec![(24, 0)], vec![(32, 120)]], vec!["maximum"]),
                (
                    0,
                    vec![vec![(24, 100), (32, 120), (150, 0x10)]],
                    vec!["alive", "health", "maximum", "flags"],
                ),
            ] {
                let mut world = world(initial)?;
                let mut state = RuntimePlayerUiState::default();
                state.refresh_health(&world);
                state.discard_published_notifications();
                let sent = server
                    .exchange(vec![(0xa9, values_packet(&blocks))], 0)
                    .await?;
                let packet = session.receive_packet().await?;
                let batch = packet.object_updates()?.ok_or("object batch")?;
                apply_object_updates_with_units::<GameplayUpdateError>(
                    &mut world,
                    &batch,
                    1000,
                    &mut |_, _, _| Ok(()),
                    &mut |world, identity, notification| {
                        state.receive_unit_field(world, identity, notification, 1000)
                    },
                )?;
                state.refresh_health(&world);
                assert_eq!(events(&mut state), expected);
                let expected_prediction = if expected == ["maximum"] {
                    initial
                } else {
                    world.local_player_vitals().ok_or("vitals")?.health()
                };
                assert_eq!(
                    world
                        .unit_health_prediction(7)
                        .ok_or("prediction")?
                        .health() as u32,
                    expected_prediction
                );
                sent.await??;
            }
            let mut world = world(100)?;
            world.update_fields(7, [(150, 0x10)])?;
            let mut state = RuntimePlayerUiState::default();
            let sent = server
                .exchange(vec![(0xa9, values_packet(&[vec![(150, 0)]]))], 0)
                .await?;
            let packet = session.receive_packet().await?;
            apply_object_updates_with_units::<GameplayUpdateError>(
                &mut world,
                &packet.object_updates()?.ok_or("batch")?,
                2000,
                &mut |_, _, _| Ok(()),
                &mut |world, identity, notification| {
                    state.receive_unit_field(world, identity, notification, 2000)
                },
            )?;
            assert_eq!(events(&mut state), ["flags", "unghost"]);
            sent.await??;
            Ok(())
        })
}

fn values_packet(blocks: &[Vec<(u16, u32)>]) -> Vec<u8> {
    let mut packet = (blocks.len() as u32).to_le_bytes().to_vec();
    for fields in blocks {
        packet.extend([0, 1, 7]); // Values block, packed local GUID.
        let words = fields
            .iter()
            .map(|(index, _)| usize::from(*index) / 32 + 1)
            .max()
            .unwrap_or(0);
        packet.push(words as u8);
        let mut mask = vec![0_u32; words];
        for (index, _) in fields {
            mask[usize::from(*index) / 32] |= 1 << (index % 32);
        }
        for word in mask {
            packet.extend(word.to_le_bytes());
        }
        let mut fields = fields.clone();
        fields.sort_unstable_by_key(|(index, _)| *index);
        for (_, value) in fields {
            packet.extend(value.to_le_bytes());
        }
    }
    packet
}

#[test]
fn player_life_lua_observes_timer_and_final_ghost_before_each_event() -> Result<(), TestError> {
    use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
    use solarity_ui::{
        AddonCatalog, FrameManager, UiPlayerState, UiPlayerVitalsState, UiScriptEnvironment,
        UiUnitPowerType,
    };
    let fixture = super::tests::fixture_with(&[
        ("Interface/FrameXML/FrameXML.toc", b"Life.xml\n"),
        ("Interface/FrameXML/Life.xml", br#"<Ui><Frame name="LifeObserver"><Scripts>
<OnLoad>LOG='';for _,e in ipairs({'PLAYER_DEAD','PLAYER_ALIVE','UNIT_HEALTH','PLAYER_FLAGS_CHANGED','PLAYER_UNGHOST'}) do self:RegisterEvent(e) end</OnLoad>
<OnEvent>LOG=LOG..event..':'..tostring(UnitIsGhost('player'))..':'..GetReleaseTimeRemaining()..'|'</OnEvent>
</Scripts></Frame></Ui>"#),
    ]).map_err(|error| error.to_string())?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?
        .with_client_clock(solarity_ui::UiClientClock::from_source(|| 1000));
    let ui = environment.world_state();
    ui.enter_player(UiPlayerState::new(0));
    ui.set_player_guid(7);
    ui.set_player_vitals(UiPlayerVitalsState::new(
        100,
        100,
        100,
        100,
        UiUnitPowerType::Mana,
    ));
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    let mut world = world(0)?;
    let mut state = RuntimePlayerUiState::default();
    let identity = world.object_identity(7).ok_or("identity")?;
    state.receive_unit_field(
        &world,
        identity,
        UnitFieldNotification::Health { previous: 100 },
        1000,
    );
    world.update_fields(7, [(24, 100), (150, 0x10)])?;
    solarity_systems::project_object_fields(&mut world, 7, [(24, 100), (150, 0x10)])?;
    state.receive_unit_field(
        &world,
        identity,
        UnitFieldNotification::Health { previous: 0 },
        1000,
    );
    state.receive_unit_field(
        &world,
        identity,
        UnitFieldNotification::PlayerFlags { previous: 0 },
        1000,
    );
    world.update_fields(7, [(150, 0)])?;
    state.receive_unit_field(
        &world,
        identity,
        UnitFieldNotification::PlayerFlags { previous: 0x10 },
        1000,
    );
    while let Some(notification) = state.take_notification() {
        match notification {
            RuntimePlayerUiNotification::Life {
                snapshot,
                event,
                release_timer,
            } => super::dispatch_life(&mut manager, &ui, snapshot, event, release_timer)?,
            RuntimePlayerUiNotification::Health {
                snapshot,
                health_changed,
                maximum_changed,
            } => super::dispatch_health(
                &mut manager,
                &ui,
                snapshot,
                health_changed,
                maximum_changed,
            )?,
            _ => panic!("life event"),
        }
    }
    assert_eq!(
        manager.localized_text("LOG")?.as_deref(),
        Some(
            "PLAYER_DEAD:nil:360|UNIT_HEALTH:nil:360|PLAYER_ALIVE:1:360|UNIT_HEALTH:1:360|PLAYER_FLAGS_CHANGED:1:360|PLAYER_FLAGS_CHANGED:nil:360|PLAYER_UNGHOST:nil:360|"
        )
    );
    assert!(manager.take_callback_failure().is_none());
    Ok(())
}
