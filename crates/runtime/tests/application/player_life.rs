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
            RuntimePlayerUiNotification::UnitDeath(_) => events.push("unit_died"),
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
fn unit_death_log_observes_native_timer_before_player_dead() -> Result<(), TestError> {
    for line in include_str!("../fixtures/unit_death_log_timer_native.txt").lines() {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let mut active = world(0)?;
        active.update_fields(
            7,
            [
                (79, u32::from_str_radix(row[0], 16)?),
                (1197, u32::from_str_radix(row[1], 16)?),
                (150, u32::from_str_radix(row[2], 16)?),
            ],
        )?;
        let mut state = RuntimePlayerUiState::default();
        state.receive_unit_field(
            &active,
            active.object_identity(7).ok_or("identity")?,
            UnitFieldNotification::Health { previous: 100 },
            1000,
        );
        let mut notification = state.take_notification();
        if matches!(
            notification,
            Some(RuntimePlayerUiNotification::Resurrection(_))
        ) {
            notification = state.take_notification();
        }
        let Some(RuntimePlayerUiNotification::UnitDeath(death)) = notification else {
            return Err("death log must precede life callbacks".into());
        };
        let (health, timer) = death.player_ui.ok_or("death UI snapshot")?;
        assert_eq!(health.health, 0);
        assert_eq!(health.predicted, 0);
        assert_eq!(timer.remaining(1000), row[3].parse::<i32>()?, "{line}");
    }
    Ok(())
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
            if row[4] == "dead" {
                vec!["unit_died", "dead", "health"]
            } else {
                vec![row[4], "health"]
            }
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
                (
                    100,
                    vec![vec![(24, 0)]],
                    vec!["unit_died", "dead", "health"],
                ),
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
<OnLoad>LOG='';for _,e in ipairs({'COMBAT_LOG_EVENT','COMBAT_LOG_EVENT_UNFILTERED','PLAYER_DEAD','CURSOR_UPDATE','PLAYER_ALIVE','UNIT_HEALTH','PLAYER_FLAGS_CHANGED','PLAYER_UNGHOST'}) do self:RegisterEvent(e) end</OnLoad>
<OnEvent>LOG=LOG..event..':'..tostring(UnitIsGhost('player'))..':'..GetReleaseTimeRemaining()..'|';if event=='COMBAT_LOG_EVENT' then HEALTH_AT_DEATH_LOG=tostring(UnitHealth('player')) end;if event=='PLAYER_DEAD' then ITEM_AT_DEATH=tostring(CursorHasItem()) elseif event=='CURSOR_UPDATE' then ITEM_AFTER_DEATH=tostring(CursorHasItem()) end</OnEvent>
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
    ui.set_cursor_has_item(true);
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
            RuntimePlayerUiNotification::UnitDeath(snapshot) => {
                super::super::environmental_damage::dispatch_unit_death(
                    &mut manager,
                    &ui,
                    snapshot,
                )?
            }
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
            "COMBAT_LOG_EVENT:nil:360|COMBAT_LOG_EVENT_UNFILTERED:nil:360|PLAYER_DEAD:nil:360|CURSOR_UPDATE:nil:360|UNIT_HEALTH:nil:360|PLAYER_ALIVE:1:360|UNIT_HEALTH:1:360|PLAYER_FLAGS_CHANGED:1:360|PLAYER_FLAGS_CHANGED:nil:360|PLAYER_UNGHOST:nil:360|"
        )
    );
    assert!(manager.take_callback_failure().is_none());
    assert_eq!(
        manager.localized_text("HEALTH_AT_DEATH_LOG")?.as_deref(),
        Some("0")
    );
    assert_eq!(
        manager.localized_text("ITEM_AT_DEATH")?.as_deref(),
        Some("1")
    );
    assert_eq!(
        manager.localized_text("ITEM_AFTER_DEATH")?.as_deref(),
        Some("nil")
    );
    Ok(())
}

#[test]
fn unit_death_log_lua_arguments_match_original_record_builder() -> Result<(), TestError> {
    use crate::application::gameplay_coordinator::{
        environmental_damage::RuntimeCombatLogClock, unit_death::RuntimeUnitDeathSnapshot,
    };
    use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
    use solarity_ui::{AddonCatalog, FrameManager, UiScriptEnvironment};
    let fixture = super::tests::fixture_with(&[
        ("Interface/FrameXML/FrameXML.toc", b"DeathLog.xml\n"),
        ("Interface/FrameXML/DeathLog.xml", br#"<Ui><Frame name="DeathLog"><Scripts>
<OnLoad>LOG='';self:RegisterEvent('COMBAT_LOG_EVENT');self:RegisterEvent('COMBAT_LOG_EVENT_UNFILTERED')</OnLoad>
<OnEvent>local values={};for i=1,select('#',...) do values[i]=tostring(select(i,...)) end;if LOG~='' then LOG=LOG..';' end;LOG=LOG..(event=='COMBAT_LOG_EVENT' and '566:' or '567:')..table.concat(values,',')</OnEvent>
</Scripts></Frame></Ui>"#),
        ("Interface/FrameXML/Bindings.xml", br#"<Bindings><Binding name="RESET">LOG='';CombatLogClearEntries()</Binding></Bindings>"#),
    ]).map_err(|error| error.to_string())?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        UiScriptEnvironment::new(800, 600, false)?,
        &[],
        &AddonCatalog::default(),
    )?;
    for line in include_str!("../fixtures/unit_death_log_native.txt").lines() {
        let row = line.split('|').collect::<Vec<_>>();
        let guid = u64::from_str_radix(row[0], 16)?;
        let mut active = world(0)?;
        active.create_object(
            guid,
            if guid <= 2 {
                ObjectKind::Player
            } else {
                ObjectKind::Unit
            },
            None,
            [],
        )?;
        manager.invoke_binding("RESET", true)?;
        super::super::environmental_damage::dispatch_unit_death(
            &mut manager,
            &solarity_ui::UiWorldState::default(),
            RuntimeUnitDeathSnapshot {
                player_ui: None,
                identity: active.object_identity(guid).ok_or("identity")?,
                name: Some("WaterTest".into()),
                flags: match guid {
                    1 => 1297,
                    2 => 1320,
                    _ => 2600,
                },
                event: if row[1] == "13" {
                    "UNIT_DISSIPATES"
                } else {
                    "UNIT_DIED"
                },
                timestamp_ms: 2250,
                clock: RuntimeCombatLogClock {
                    unix_seconds: 1_700_000_000,
                    milliseconds: 1000,
                },
            },
        )?;
        assert_eq!(
            manager.localized_text("LOG")?.as_deref(),
            Some(row[2]),
            "{line}"
        );
    }
    assert!(manager.take_callback_failure().is_none());
    Ok(())
}

#[test]
fn player_life_world_entry_preserves_timer_and_sends_automatic_release_before_life()
-> Result<(), TestError> {
    use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
    use solarity_ui::{
        AddonCatalog, FrameManager, UiPlayerDeathAction, UiPlayerReleaseTimer, UiPlayerState,
        UiPlayerVitalsState, UiScriptEnvironment, UiUnitPowerType,
    };
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let fixture = super::tests::fixture_with(&[
            ("Interface/FrameXML/FrameXML.toc", b"Entry.xml\n"),
            ("Interface/FrameXML/Entry.xml", br#"<Ui><Frame name="EntryObserver"><Scripts>
<OnLoad>LOG='';for _,e in ipairs({'PLAYER_ENTERING_WORLD','PLAYER_DEAD','CURSOR_UPDATE','PLAYER_ALIVE'}) do self:RegisterEvent(e) end</OnLoad>
<OnEvent>if event=='PLAYER_ENTERING_WORLD' then LOG='' end;LOG=LOG..event..'|'</OnEvent>
</Scripts></Frame></Ui>"#),
        ]).map_err(|error| error.to_string())?;
        let store = AssetStore::mount(ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?)?;
        let environment = UiScriptEnvironment::new(800, 600, false)?;
        let ui = environment.world_state();
        ui.enter_player(UiPlayerState::new(0));
        let timer = UiPlayerReleaseTimer::on_death(8, 0, 1000);
        ui.set_release_timer(timer);
        let mut manager = FrameManager::start_shared(AssetStoreHandle::new(store), environment, &[], &AddonCatalog::default())?;
        let (server, session) = WorldServer::connect().await?;
        let (_, mut writer) = session.split();
        for row in include_str!("../fixtures/player_life_native.txt").lines().filter(|line| line.starts_with("health ")).map(|line| line.split_ascii_whitespace().collect::<Vec<_>>()) {
            let health = u32::from_str_radix(row[2], 16)?;
            // A ghost with positive raw health emits ALIVE and does not auto-release.
            for ghost in [false, true] {
                ui.set_player_vitals(UiPlayerVitalsState::new(health, 100, 0, 0, UiUnitPowerType::Mana).with_health(health, 100, 12345, ghost));
                manager.dispatch_event("PLAYER_ENTERING_WORLD", &solarity_ui::UiEventPayload::empty())?;
                super::dispatch_world_entry_life(&mut manager, &ui)?;
                let expected = if row[5] == "102" { "PLAYER_ENTERING_WORLD|PLAYER_DEAD|CURSOR_UPDATE|" } else { "PLAYER_ENTERING_WORLD|PLAYER_ALIVE|" };
                assert_eq!(manager.localized_text("LOG")?.as_deref(), Some(expected));
                assert_eq!(row[6], if row[5] == "102" { "113" } else { "none" });
                assert_eq!(ui.release_timer(), timer);
                if row[5] == "102" {
                    assert_eq!(ui.pending_death_action(), Some(UiPlayerDeathAction::ReleaseSpirit { automatic: true }));
                    let received = server.exchange_raw(vec![], 1).await?;
                    writer.send_release_spirit(true).await?;
                    ui.accept_death_action();
                    assert_eq!(received.await??, vec![(0x15a, vec![1])]);
                }
                assert_eq!(ui.pending_death_action(), None);
            }
        }
        ui.set_player_vitals(UiPlayerVitalsState::new(0, 100, 0, 0, UiUnitPowerType::Mana));
        ui.player_entered_world();
        assert!(ui.pending_death_action().is_some());
        ui.clear_death_actions();
        assert_eq!(ui.pending_death_action(), None);
        assert_eq!(ui.release_timer(), timer);
        assert_eq!(manager.take_callback_failure(), None);
        Ok(())
    })
}
