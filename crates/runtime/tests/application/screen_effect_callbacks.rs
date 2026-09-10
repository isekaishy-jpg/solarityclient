//! Native callback admission compared with real packet decoding and Spell columns.

use super::*;
use crate::test_network::{TestError, WorldServer};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, SpellEffectCatalog};
use solarity_ecs::UnitAuras;

#[test]
fn screen_effect_aura_callbacks_match_native() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut table = b"WDBC".to_vec();
            table.extend([7u32, 234, 936, 1].into_iter().flat_map(u32::to_le_bytes));
            for (index, (types, misc, priority, visibility)) in [
                ([0, 0, 0], [0, 0, 0], 0, 0),
                ([260, 0, 0], [81, 0, 0], 0, 0),
                ([260, 260, 0], [141, 242, 0], 0, 0),
                ([0, 0, 260], [0, 0, 0], 0, 0),
                ([19, 260, 0], [0, 141, 0], 0, 0),
                ([260, 0, 0], [242, 0, 0], 5, 0),
                ([260, 0, 0], [342, 0, 0], 0, 3),
            ]
            .into_iter()
            .enumerate()
            {
                let mut row = [0u32; 234];
                row[0] = 100 + index as u32;
                row[95..98].copy_from_slice(&types);
                row[110..113].copy_from_slice(&misc);
                row[135] = priority;
                row[221] = visibility;
                table.extend(row.into_iter().flat_map(u32::to_le_bytes));
            }
            table.push(0);
            let fixture = crate::test_support::ClientFixture::with_common_files(&[(
                "DBFilesClient/Spell.dbc",
                &table,
            )])
            .map_err(|error| error.to_string())?;
            let mut assets = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let spells = SpellEffectCatalog::load(&mut assets)?;
            assert_eq!(spells.spell(105).ok_or("spell")?.visual_priority, 5);
            assert_eq!(spells.spell(106).ok_or("spell")?.required_aura_vision, 3);
            let lookup = |id| spells.spell(id).copied();
            let rows = include_str!("../fixtures/screen_effect_callbacks_native.txt")
                .lines()
                .filter_map(|line| line.strip_prefix("callback "))
                .map(|line| line.split_whitespace().collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let packets = rows
                .iter()
                .map(|row| {
                    let opcode = row[6].parse::<u16>()?;
                    let body = (0..row[7].len())
                        .step_by(2)
                        .map(|index| u8::from_str_radix(&row[7][index..index + 2], 16))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok((opcode, body))
                })
                .collect::<Result<Vec<_>, std::num::ParseIntError>>()?;
            let (server, mut network) = WorldServer::connect().await?;
            let sent = server.exchange(packets, 0).await?;
            for row in &rows {
                let update = network
                    .receive_packet()
                    .await?
                    .unit_auras()?
                    .ok_or("auras")?;
                let mut before = [UnitAura::default(); 8];
                before[3].spell = row[0].parse()?;
                before[3].flags = row[1].parse()?;
                let mut current = UnitAuras::default();
                for (index, aura) in before.iter().copied().enumerate() {
                    current.set(index as u8, aura);
                }
                if update.replace {
                    current.clear();
                }
                let mut touched = [update.replace; 256];
                for aura in update.auras {
                    touched[aura.slot as usize] = true;
                    current.set(
                        aura.slot,
                        UnitAura {
                            spell: aura.spell,
                            flags: aura.flags,
                            ..Default::default()
                        },
                    );
                }
                let mut state = AuraCallbacks {
                    visual_spells: vec![0; 8],
                };
                state.visual_spells[3] = row[2].parse()?;
                let bytes = row[4].parse::<u32>()? << 24;
                let count = state.receive(
                    &before,
                    current.slots(),
                    &touched,
                    update.replace,
                    bytes,
                    lookup,
                );
                assert_eq!(count, row[8].parse::<usize>()?, "{row:?}");
                assert_eq!(
                    state.visual_spells[3],
                    row.last().ok_or("visual")?.parse::<u32>()?,
                    "{row:?}"
                );
                let selected = super::super::select(
                    current.slots(),
                    row[3].parse::<u32>()? * 16,
                    bytes,
                    row[5] != "0",
                    lookup,
                );
                for value in &row[9..9 + count] {
                    assert_eq!(selected, value.parse::<u32>()?, "{row:?}");
                }
            }
            sent.await??;
            assert_eq!(rows.len(), 1344);
            let mut vision_cases = 0;
            for line in include_str!("../fixtures/screen_effect_callbacks_native.txt")
                .lines()
                .filter_map(|line| line.strip_prefix("vision "))
            {
                let row = line
                    .split_whitespace()
                    .map(str::parse::<u32>)
                    .collect::<Result<Vec<_>, _>>()?;
                let mut current = [UnitAura::default(); 8];
                current[3].spell = row[0];
                current[3].flags = row[1] as u8;
                let mut state = AuraCallbacks {
                    visual_spells: vec![0; 8],
                };
                state.visual_spells[3] = row[2];
                let count = state.vision_changed(&current, row[3] as u8, row[4] << 24, lookup);
                assert_eq!(count, row[5] as usize, "{line}");
                assert_eq!(
                    state.visual_spells[3],
                    *row.last().ok_or("visual")?,
                    "{line}"
                );
                let selected = super::super::select(&current, 16, row[4] << 24, false, lookup);
                for &expected in &row[6..6 + count] {
                    assert_eq!(selected, expected, "{line}");
                }
                vision_cases += 1;
            }
            assert_eq!(vision_cases, 640);
            Ok(())
        })
}
