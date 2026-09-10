//! Original 4F88B0 results cover priority, inactive slots and player fallbacks.

use super::*;
use std::error::Error;

fn table<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    bytes.extend(
        [
            rows.len() as u32,
            N as u32,
            N as u32 * 4,
            strings.len() as u32,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes),
    );
    bytes.extend(rows.iter().flatten().flat_map(|v| v.to_le_bytes()));
    bytes.extend_from_slice(strings);
    bytes
}

#[test]
fn screen_effect_dispatch_follows_watched_fields_and_world_lifetime()
-> Result<(), crate::test_network::TestError> {
    use crate::application::gameplay_session::{
        GameplayUpdateError, apply_object_updates_with_units,
    };
    use crate::test_network::WorldServer;
    use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(0),
                7,
                "Effects",
                glam::Vec3::ZERO,
                0.,
            ));
            world.create_object(7, ObjectKind::Player, None, [(150, 0), (1229, 0)])?;
            world.create_object(8, ObjectKind::Player, None, [(150, 0), (1229, 0)])?;
            let mut spell = [0u32; 234];
            spell[0] = 10;
            spell[95] = 260;
            spell[110] = 141;
            spell[221] = 3;
            let fixture = crate::test_support::ClientFixture::with_common_files(&[(
                "DBFilesClient/Spell.dbc",
                &table(&[spell], b"\0"),
            )])
            .map_err(|e| e.to_string())?;
            let mut assets =
                solarity_asset::AssetStore::mount(solarity_asset::ArchiveCatalog::discover(
                    solarity_asset::ClientDataRoot::new(fixture.data_root())?,
                    solarity_asset::Locale::EnUs,
                )?)?;
            let spells = std::rc::Rc::new(solarity_asset::SpellEffectCatalog::load(&mut assets)?);
            let mut gameplay = super::super::RuntimeGameplayCoordinator::with_test_world(world)
                .with_spells(spells);
            gameplay.player_ui.screen_effect.initialize(
                gameplay.world.as_ref().ok_or("world")?,
                None,
                false,
            );
            assert_eq!(gameplay.take_screen_effect_update(), Some(0));
            for (step, (guid, fields, expected)) in [
                (7, vec![(1229, 0x40000000)], vec![81]),
                (7, vec![(1229, 0x40000000)], vec![]),
                (7, vec![(1229, 0x40000001)], vec![]),
                (7, vec![(150, 16)], vec![1]),
                (7, vec![(150, 17)], vec![]),
                (7, vec![(150, 1)], vec![81]),
                (7, vec![(1229, 1)], vec![0]),
                (7, vec![(150, 16), (1229, 0x40000000)], vec![1, 1]),
                (8, vec![(1229, 0x40000000)], vec![1]),
                (8, vec![(1229, 0)], vec![1]),
                (7, vec![(1229, 0x44000000)], vec![141]),
                (8, vec![(1229, 0x04000000)], vec![]),
                (7, vec![(1229, 0x40000000)], vec![141]),
                (8, vec![(1229, 0)], vec![141]),
            ]
            .into_iter()
            .enumerate()
            {
                if step == 10 {
                    let world = gameplay.world.as_mut().ok_or("world")?;
                    let mut auras = UnitAuras::default();
                    auras.set(
                        3,
                        UnitAura {
                            spell: 10,
                            flags: 8,
                            ..Default::default()
                        },
                    );
                    let player = world.local_player();
                    world.storage_mut().add_component(player, (auras,));
                }
                let mut body = 1u32.to_le_bytes().to_vec();
                body.extend([0, 1, guid]); // Values block and packed player GUID.
                let count = fields
                    .iter()
                    .map(|(field, _)| field / 32 + 1)
                    .max()
                    .ok_or("fields")?;
                body.push(count as u8);
                let mut mask = vec![0u32; count];
                for (field, _) in &fields {
                    mask[field / 32] |= 1 << (field % 32);
                }
                body.extend(mask.into_iter().flat_map(u32::to_le_bytes));
                body.extend(
                    fields
                        .iter()
                        .flat_map(|(_, value): &(usize, u32)| value.to_le_bytes()),
                );
                server.exchange(vec![(0xa9, body)], 0).await?.await??;
                let batch = network
                    .receive_packet()
                    .await?
                    .object_updates()?
                    .ok_or("updates")?;
                apply_object_updates_with_units::<GameplayUpdateError>(
                    gameplay.world.as_mut().ok_or("world")?,
                    &batch,
                    1000,
                    &mut |_, _, _| Ok(()),
                    &mut |world, identity, event| {
                        gameplay
                            .player_ui
                            .receive_unit_field(world, identity, event, 1000)
                    },
                )?;
                let actual =
                    std::iter::from_fn(|| gameplay.take_screen_effect_update()).collect::<Vec<_>>();
                assert_eq!(actual, expected, "{fields:?}");
            }
            gameplay.player_ui.arena = true;
            assert_eq!(gameplay.screen_effect_id(), 141);
            assert_eq!(gameplay.take_screen_effect_update(), None);
            server
                .exchange(vec![(0x236, vec![0; 20])], 0)
                .await?
                .await??;
            let location = network
                .receive_packet()
                .await?
                .world_location()?
                .ok_or("location")?;
            gameplay.replace_world(location)?;
            assert_eq!(gameplay.screen_effect_id(), 0);
            assert_eq!(gameplay.take_screen_effect_update(), Some(0));
            assert_eq!(gameplay.take_screen_effect_update(), None);
            assert!(
                gameplay
                    .player_ui
                    .screen_effect
                    .callbacks
                    .visual_spells
                    .is_empty()
            );
            gameplay.disconnect();
            assert_eq!(gameplay.screen_effect_id(), 0);
            assert_eq!(gameplay.take_screen_effect_update(), None);
            Ok(())
        })
}

#[test]
fn screen_effect_reads_authored_misc_values_and_live_player_slots() -> Result<(), Box<dyn Error>> {
    use solarity_asset::{
        ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, SpellEffectCatalog,
    };
    use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
    let mut spell = [0u32; 234];
    spell[0] = 10;
    spell[96] = 260;
    spell[111] = 141;
    spell[112] = 999;
    let mut map = [0u32; 66];
    map[0] = 42;
    map[1] = 1;
    map[2] = 4;
    let fixture = crate::test_support::ClientFixture::with_common_files(&[
        ("DBFilesClient/Spell.dbc", &table(&[spell], b"\0")),
        ("DBFilesClient/Map.dbc", &table(&[map], b"\0Arena\0")),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let spells = std::rc::Rc::new(SpellEffectCatalog::load(&mut store)?);
    let maps = MapCatalog::load(&mut store)?;
    for (map_id, expected) in [(0, 1), (42, 81)] {
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(map_id),
            7,
            "Screen",
            glam::Vec3::ZERO,
            0.,
        ));
        world.create_object(7, ObjectKind::Player, None, [(150, 16), (1229, 0x40000000)])?;
        let mut gameplay = super::super::RuntimeGameplayCoordinator::with_test_world(world)
            .with_maps(&maps)
            .with_spells(spells.clone());
        assert_eq!(gameplay.screen_effect_id(), 1); // World Map.dbc kind alone has no effect.
        gameplay.battlefield.receive(
            solarity_network::WorldBattlefieldStatus::Status {
                queue: 0,
                status: 3,
                map: Some(map_id),
            },
            &gameplay.battlefield_maps,
            &mut gameplay.player_ui.arena,
        );
        assert_eq!(gameplay.screen_effect_id(), 1); // Battlefield updates do not call the selector.
        gameplay.player_ui.screen_effect.refresh(
            gameplay.world.as_ref().ok_or("world")?,
            Some(&spells),
            gameplay.player_ui.arena,
        );
        assert_eq!(gameplay.screen_effect_id(), expected);
        gameplay.player_ui.receive_auras(
            gameplay.world.as_mut().ok_or("world")?,
            solarity_network::WorldUnitAuraUpdate {
                guid: 7,
                replace: true,
                auras: vec![solarity_network::WorldUnitAura {
                    slot: 3,
                    spell: 10,
                    flags: 1,
                    level: 80,
                    applications: 1,
                    caster: 7,
                    duration: None,
                }],
            },
            100,
        );
        assert_eq!(gameplay.screen_effect_id(), 141); // Native ignores effect-enable flag one here.
        gameplay.player_ui.receive_auras(
            gameplay.world.as_mut().ok_or("world")?,
            solarity_network::WorldUnitAuraUpdate {
                guid: 7,
                replace: true,
                auras: vec![],
            },
            200,
        );
        assert_eq!(gameplay.screen_effect_id(), expected);
    }
    Ok(())
}

#[test]
fn screen_effect_selection_matches_native() -> Result<(), Box<dyn Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/screen_effect_native.txt").lines() {
        let Some(line) = line.strip_prefix("select ") else {
            continue;
        };
        let row = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let auras = row[6..6 + row[5] as usize]
            .iter()
            .map(|&spell| UnitAura {
                spell,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let output = if row[0] == 0 {
            0
        } else {
            select(
                &auras,
                row[2] * 16,
                row[3] * 0x40000000,
                row[4] != 0,
                |id| {
                    let (aura_types, misc_values) = match id {
                        1 => ([260, 260, 260], [141, 242, 0]),
                        2 => ([0, 260, 260], [999, 242, 141]),
                        3 => ([0, 0, 260], [999, 999, 0]),
                        4 => ([0, 0, 0], [141, 242, 141]),
                        5 => ([0, 0, 260], [0, 0, u32::MAX]),
                        _ => return None,
                    };
                    Some(SpellEffectDefinition {
                        effects: [0; 3],
                        aura_types,
                        misc_values,
                        visual_priority: 0,
                        required_aura_vision: 0,
                        resurrection_bypass: false,
                    })
                },
            )
        };
        assert_eq!(output, *row.last().ok_or("missing result")?, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 448);
    Ok(())
}
