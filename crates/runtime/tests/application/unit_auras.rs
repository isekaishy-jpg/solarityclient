//! Native aura slot replacement and enabled resurrection effects through encryption.

use super::player_ui::RuntimePlayerUiState;
use crate::test_network::{TestError, WorldServer};
use crate::test_support::ClientFixture;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, Locale, SpellEffectCatalog, SpellNameCatalog,
};
use solarity_ecs::{ActiveWorld, ObjectKind, UnitAuras, WorldBootstrap, WorldMapId};
use std::rc::Rc;

fn spell_table() -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [5_u32, 234, 936, 1] {
        bytes.extend(value.to_le_bytes());
    }
    for index in 0..5 {
        let mut record = [0_u32; 234];
        record[0] = 100 + index as u32;
        if (1..=3).contains(&index) {
            record[95 + index - 1] = 314;
        }
        if index == 4 {
            record[11] = 0x08000000;
        }
        for word in record {
            bytes.extend(word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}

#[test]
fn forced_release_matches_native_callback_gates_and_packet_order() -> Result<(), TestError> {
    use super::player_ui::RuntimePlayerUiNotification;
    let table = spell_table();
    let fixture = ClientFixture::with_common_files(&[("DBFilesClient/Spell.dbc", &table)])
        .map_err(|e| e.to_string())?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let spells = Rc::new(SpellEffectCatalog::load(&mut assets)?);
    let mut count = 0;
    for line in include_str!("../fixtures/player_force_release_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = line.split_whitespace().collect::<Vec<_>>();
        let health = u32::from_str_radix(row[1], 16)?;
        if (health as i32) > 0 {
            continue;
        } // 729220 enters only for raw-health death.
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "ForceRelease",
            glam::Vec3::ZERO,
            0.0,
        ));
        let fields = [
            (24, health),
            (32, 100),
            (1197, 8),
            (150, u32::from_str_radix(row[2], 16)?),
            (79, u32::from_str_radix(row[3], 16)?),
            (59, u32::from_str_radix(row[4], 16)?),
        ];
        world.create_object(7, ObjectKind::Player, None, fields)?;
        solarity_systems::project_object_fields(&mut world, 7, fields)?;
        world.create_object(8, ObjectKind::Unit, None, fields)?;
        solarity_systems::project_object_fields(&mut world, 8, fields)?;
        let mut state = RuntimePlayerUiState::default();
        state.set_spells(Some(spells.clone()));
        if row[5] == "0" {
            state.receive_auras(
                &mut world,
                solarity_network::WorldUnitAuraUpdate {
                    guid: 7,
                    replace: true,
                    auras: vec![solarity_network::WorldUnitAura {
                        slot: 0,
                        spell: 101,
                        flags: 9,
                        level: 80,
                        applications: 1,
                        caster: 7,
                        duration: None,
                    }],
                },
                1000,
            );
            state.discard_published_notifications();
        }
        let guid = if row[0] == "1" { 7 } else { 8 };
        state.receive_unit_field(
            &world,
            world.object_identity(guid).ok_or("identity")?,
            crate::application::gameplay_session::UnitFieldNotification::Health { previous: 100 },
            1000,
        );
        let mut actions = Vec::new();
        let mut death_seen = false;
        while let Some(notification) = state.take_notification() {
            match notification {
                RuntimePlayerUiNotification::DeathAction(action) => {
                    assert!(!death_seen, "forced release precedes the death record");
                    actions.push(action);
                }
                RuntimePlayerUiNotification::UnitDeath(_) => death_seen = true,
                _ => {}
            }
        }
        let expected = if row[6].contains("15a,byte0") {
            vec![solarity_ui::UiPlayerDeathAction::ReleaseSpirit { automatic: false }]
        } else {
            vec![]
        };
        assert_eq!(actions, expected, "{line}");
        assert_eq!(
            state.release_timer().remaining(1000),
            if row[6].starts_with("timer") { 360 } else { 0 },
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 64);
    Ok(())
}

#[test]
fn encrypted_aura_slots_and_resurrection_admission_match_native() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let table = spell_table();
            let fixture = ClientFixture::with_common_files(&[("DBFilesClient/Spell.dbc", &table)])
                .map_err(|e| e.to_string())?;
            let mut assets = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let spells = Rc::new(SpellEffectCatalog::load(&mut assets)?);
            let names = SpellNameCatalog::load(&mut assets)?;
            let mut state = RuntimePlayerUiState::default();
            state.set_spells(Some(spells));
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(0),
                7,
                "AuraTest",
                glam::Vec3::ZERO,
                0.0,
            ));
            world.create_object(7, ObjectKind::Player, None, [(24, 0), (32, 100)])?;
            solarity_systems::project_object_fields(&mut world, 7, [(24, 0), (32, 100)])?;
            let ui = solarity_ui::UiWorldState::default();
            ui.enter_player(solarity_ui::UiPlayerState::new(0));
            let (server, mut network) = WorldServer::connect().await?;
            let rows = include_str!("../fixtures/unit_aura_native.txt")
                .lines()
                .filter(|line| !line.starts_with('#'))
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 120);
            for group in rows.as_chunks::<12>().0 {
                let first = group[0].split_whitespace().collect::<Vec<_>>();
                let opcode = u16::from_str_radix(first[0], 16)?;
                let now = first[1].parse::<u32>()?;
                let body = first[2]
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
                    .collect::<Result<Vec<_>, TestError>>()?;
                server
                    .exchange(vec![(opcode, body.clone())], 0)
                    .await?
                    .await??;
                let update = network
                    .receive_packet()
                    .await?
                    .unit_auras()?
                    .ok_or("auras")?;
                let mut boundaries = vec![2];
                for aura in &update.auras {
                    let mut length = 5;
                    if aura.spell != 0 {
                        length += 3;
                        if aura.flags & 8 == 0 {
                            length += 1 + aura
                                .caster
                                .to_le_bytes()
                                .iter()
                                .filter(|byte| **byte != 0)
                                .count();
                        }
                        if aura.duration.is_some() {
                            length += 8;
                        }
                    }
                    boundaries.push(boundaries.last().copied().ok_or("boundary")? + length);
                }
                state.receive_auras(&mut world, update, now);
                for line in group {
                    let row = line.split_whitespace().collect::<Vec<_>>();
                    let slot = row[3].parse::<u8>()?;
                    let actual = world
                        .storage()
                        .get::<&UnitAuras>(world.local_player())?
                        .slot(slot);
                    assert_eq!(
                        [
                            actual.spell,
                            u32::from(actual.flags),
                            u32::from(actual.level),
                            u32::from(actual.applications),
                            actual.duration_ms,
                            actual.end_ms
                        ],
                        [
                            row[4].parse::<u32>()?,
                            row[5].parse::<u32>()?,
                            row[6].parse::<u32>()?,
                            row[7].parse::<u32>()?,
                            row[9].parse::<u32>()?,
                            row[10].parse::<u32>()?
                        ],
                        "{line}"
                    );
                    assert_eq!(actual.caster, u64::from_str_radix(row[8], 16)?, "{line}");
                    world.update_fields(7, [(1199, row[12].parse()?)])?;
                    state.refresh_health(&world);
                    state.resurrection().publish(&ui, &names);
                    assert_eq!(ui.resurrection_state().blocked, row[11] == "1", "{line}");
                    assert_eq!(
                        ui.resurrection_state().release_allowed(),
                        row[13] == "1",
                        "{line}"
                    );
                }
                // Truncation inside the packed GUID or any record is rejected;
                // a prefix ending at a record boundary is a valid shorter delta.
                for length in 0..body.len() {
                    server
                        .exchange(vec![(opcode, body[..length].to_vec())], 0)
                        .await?
                        .await??;
                    let packet = network.receive_packet().await?;
                    assert_eq!(
                        packet.unit_auras().is_ok(),
                        boundaries.contains(&length),
                        "prefix {length} of {}",
                        body.len()
                    );
                }
                state.discard_published_notifications();
            }
            world.create_object(9, ObjectKind::Unit, None, [])?;
            let update = solarity_network::WorldUnitAuraUpdate {
                guid: 9,
                replace: true,
                auras: vec![solarity_network::WorldUnitAura {
                    slot: 255,
                    spell: 101,
                    flags: 9,
                    level: 80,
                    applications: 1,
                    caster: 9,
                    duration: None,
                }],
            };
            state.receive_auras(&mut world, update, 100);
            world.remove_object(9)?;
            world.create_object(9, ObjectKind::Unit, None, [])?;
            assert!(
                world
                    .storage()
                    .get::<&UnitAuras>(world.entity_by_guid(9).ok_or("unit")?)
                    .is_err()
            );
            Ok(())
        })
}
