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
