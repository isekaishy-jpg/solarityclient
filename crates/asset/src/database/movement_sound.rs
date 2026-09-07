//! Movement sound joins from build 12340's UnitSound_C and SoundInterface.

use std::collections::BTreeMap;

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

/// Authored creature movement sounds (`CreatureSoundData.dbc`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatureMovementSounds {
    footsteps: u32,
    jump: u32,
    land: u32,
    child: u32,
}

impl CreatureMovementSounds {
    /// Returns the FootstepTerrainLookup creature selector, not a SoundEntries ID.
    #[must_use]
    pub const fn footsteps(self) -> u32 {
        self.footsteps
    }

    /// Returns the forced jump vocal selected by native `0x007461E0`, kind 11.
    #[must_use]
    pub const fn jump(self) -> u32 {
        self.jump
    }

    /// Returns the forced landing vocal selected by native `0x007461E0`, kind 12.
    #[must_use]
    pub const fn land(self) -> u32 {
        self.land
    }

    /// Returns the optional mounted child row used by native `0x0071A3F0`.
    #[must_use]
    pub const fn child(self) -> u32 {
        self.child
    }
}

/// Exact creature, terrain, and armor sound lookup tables used by movement.
pub struct MovementSoundCatalog {
    creatures: BTreeMap<u32, CreatureMovementSounds>,
    terrain_sounds: BTreeMap<u32, u32>,
    footsteps: BTreeMap<(u32, u32), [u32; 2]>,
    armor: BTreeMap<u32, u32>,
    ground_effects: BTreeMap<u32, u32>,
    liquids: super::LiquidTypeCatalog,
    areas: super::AreaTableCatalog,
}

impl MovementSoundCatalog {
    /// Loads build 12340's movement and surface sound tables through MPQ precedence.
    ///
    /// # Errors
    /// Rejects unavailable tables, incorrect layouts, and duplicate primary keys.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let mut creatures = BTreeMap::new();
        for row in rows::<38>(store, "CreatureSoundData")? {
            creatures.insert(
                row[0],
                CreatureMovementSounds {
                    footsteps: row[9],
                    jump: row[26],
                    land: row[27],
                    child: row[37],
                },
            );
        }
        let terrain_sounds = rows::<6>(store, "TerrainType")?
            .into_iter()
            .map(|row| (row[0], row[4]))
            .collect();
        let mut footsteps = BTreeMap::new();
        // 0x004CF990 traverses the table backwards, so the first authored row
        // wins if multiple records name the same creature/sound selector pair.
        for row in rows::<5>(store, "FootstepTerrainLookup")?.into_iter().rev() {
            if (row[2] as i32) >= 0 {
                footsteps.insert((row[1], row[2]), [row[3], row[4]]);
            }
        }
        // 4CFC10's AD41A8 table uses vtable A283C0 -> loader 6489C0 ->
        // filename getter 8B47D0 (Material.dbc), with foley at row +8.
        let armor = rows::<5>(store, "Material")?
            .into_iter()
            .map(|row| (row[0], row[2]))
            .collect();
        let ground_effects = rows::<11>(store, "GroundEffectTexture")?
            .into_iter()
            .map(|row| (row[0], row[10]))
            .collect();
        let liquids = super::LiquidTypeCatalog::load(store)?;
        let areas = super::AreaTableCatalog::load(store)?;
        Ok(Self {
            creatures,
            terrain_sounds,
            footsteps,
            armor,
            ground_effects,
            liquids,
            areas,
        })
    }

    /// Finds the exact creature sound row referenced by a display/model.
    #[must_use]
    pub fn creature(&self, id: u32) -> Option<CreatureMovementSounds> {
        self.creatures.get(&id).copied()
    }

    /// Resolves dry/wet footsteps, including native `0x004CF170`'s terrain-zero retry.
    #[must_use]
    pub fn footstep(&self, creature: u32, terrain: u32, wet: bool) -> u32 {
        let lookup = |terrain| {
            self.terrain_sounds
                .get(&terrain)
                .and_then(|sound| self.footsteps.get(&(creature, *sound)))
                .map_or(0, |sounds| sounds[usize::from(wet)])
        };
        let sound = lookup(terrain);
        if sound == 0 { lookup(0) } else { sound }
    }

    /// Returns Material's foley column selected by native `0x004CFC10`.
    #[must_use]
    pub fn armor(&self, material: u32) -> u32 {
        self.armor.get(&material).copied().unwrap_or(0)
    }

    /// Resolves MCLY's effect to TerrainType via native `0x007A0530`.
    #[must_use]
    pub fn ground_effect_terrain(&self, effect: u32) -> Option<u32> {
        self.ground_effects.get(&effect).copied()
    }

    /// Resolves native 0x009905C0's area/parent substitution before liquid flags.
    #[must_use]
    pub fn liquid_flags(&self, area: u32, liquid: u32) -> Option<u32> {
        self.areas.liquid_flags(&self.liquids, area, liquid)
    }
}

fn rows<const N: usize>(store: &mut AssetStore, name: &str) -> Result<Vec<[u32; N]>, AssetError> {
    let table = WdbcTable::load(
        store,
        &AssetPath::new(format!("DBFilesClient\\{name}.dbc"))?,
    )?;
    if table.header().field_count() != N as u32 || table.header().record_size() != N as u32 * 4 {
        return Err(database_error(
            &table,
            format!("build-12340 {name}.dbc requires {N} four-byte fields"),
        ));
    }
    let mut rows = Vec::with_capacity(table.header().record_count() as usize);
    let mut keys = std::collections::BTreeSet::new();
    for index in 0..table.header().record_count() {
        let mut row = [0; N];
        for (column, value) in row.iter_mut().enumerate() {
            *value = table
                .field_u32(index, column as u32)
                .ok_or_else(|| database_error(&table, format!("truncated record {index}")))?;
        }
        if !keys.insert(row[0]) {
            return Err(database_error(
                &table,
                format!("duplicate primary key {}", row[0]),
            ));
        }
        rows.push(row);
    }
    Ok(rows)
}
