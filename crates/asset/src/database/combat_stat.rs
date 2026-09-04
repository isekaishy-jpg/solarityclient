//! Build-12340 game-table coefficients used by paper-doll combat statistics.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

const MELEE_CRIT_PATH: &str = "DBFilesClient\\gtChanceToMeleeCrit.dbc";
const MELEE_CRIT_BASE_PATH: &str = "DBFilesClient\\gtChanceToMeleeCritBase.dbc";
const SPELL_CRIT_PATH: &str = "DBFilesClient\\gtChanceToSpellCrit.dbc";
const SPELL_CRIT_BASE_PATH: &str = "DBFilesClient\\gtChanceToSpellCritBase.dbc";
const OCT_HEALTH_REGEN_PATH: &str = "DBFilesClient\\gtOCTRegenHP.dbc";
const HEALTH_REGEN_PER_SPIRIT_PATH: &str = "DBFilesClient\\gtRegenHPPerSpt.dbc";
const MANA_REGEN_PER_SPIRIT_PATH: &str = "DBFilesClient\\gtRegenMPPerSpt.dbc";
const CLASS_SLOT_COUNT: u32 = 11;
const LEVEL_SLOT_COUNT: u32 = 100;

/// Client-authored coefficients for `GetCritChanceFromAgility`.
///
/// Build 12340 stores one base value per numeric class slot and one
/// per-agility value for each class/level pair. Keeping the authored `f32`
/// values here prevents FrameXML from owning combat-formula data.
pub struct CombatStatCatalog {
    melee_base_by_class: Vec<f32>,
    melee_per_agility_by_class_level: Vec<f32>,
    spell_base_by_class: Vec<f32>,
    spell_per_intellect_by_class_level: Vec<f32>,
    oct_health_regen_by_class_level: Vec<f32>,
    health_regen_per_spirit_by_class_level: Vec<f32>,
    mana_regen_per_spirit_by_class_level: Vec<f32>,
}

impl CombatStatCatalog {
    /// Loads and validates both exact game tables through archive precedence.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when either table is absent, malformed, or does
    /// not have the build-12340 one-field layout and dimensions.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let base = WdbcTable::load(store, &AssetPath::new(MELEE_CRIT_BASE_PATH)?)?;
        require_layout(&base, CLASS_SLOT_COUNT, "gtChanceToMeleeCritBase.dbc")?;
        let coefficients = WdbcTable::load(store, &AssetPath::new(MELEE_CRIT_PATH)?)?;
        require_layout(
            &coefficients,
            CLASS_SLOT_COUNT * LEVEL_SLOT_COUNT,
            "gtChanceToMeleeCrit.dbc",
        )?;
        let spell_base = WdbcTable::load(store, &AssetPath::new(SPELL_CRIT_BASE_PATH)?)?;
        require_layout(&spell_base, CLASS_SLOT_COUNT, "gtChanceToSpellCritBase.dbc")?;
        let spell_coefficients = WdbcTable::load(store, &AssetPath::new(SPELL_CRIT_PATH)?)?;
        require_layout(
            &spell_coefficients,
            CLASS_SLOT_COUNT * LEVEL_SLOT_COUNT,
            "gtChanceToSpellCrit.dbc",
        )?;
        let oct_health_regen = WdbcTable::load(store, &AssetPath::new(OCT_HEALTH_REGEN_PATH)?)?;
        require_layout(
            &oct_health_regen,
            CLASS_SLOT_COUNT * LEVEL_SLOT_COUNT,
            "gtOCTRegenHP.dbc",
        )?;
        let health_regen_per_spirit =
            WdbcTable::load(store, &AssetPath::new(HEALTH_REGEN_PER_SPIRIT_PATH)?)?;
        require_layout(
            &health_regen_per_spirit,
            CLASS_SLOT_COUNT * LEVEL_SLOT_COUNT,
            "gtRegenHPPerSpt.dbc",
        )?;
        let mana_regen_per_spirit =
            WdbcTable::load(store, &AssetPath::new(MANA_REGEN_PER_SPIRIT_PATH)?)?;
        require_layout(
            &mana_regen_per_spirit,
            CLASS_SLOT_COUNT * LEVEL_SLOT_COUNT,
            "gtRegenMPPerSpt.dbc",
        )?;

        Ok(Self {
            melee_base_by_class: float_records(&base)?,
            melee_per_agility_by_class_level: float_records(&coefficients)?,
            spell_base_by_class: float_records(&spell_base)?,
            spell_per_intellect_by_class_level: float_records(&spell_coefficients)?,
            oct_health_regen_by_class_level: float_records(&oct_health_regen)?,
            health_regen_per_spirit_by_class_level: float_records(&health_regen_per_spirit)?,
            mana_regen_per_spirit_by_class_level: float_records(&mana_regen_per_spirit)?,
        })
    }

    /// Computes the stock percentage contribution for a class, level, and
    /// current Agility value.
    ///
    /// Invalid class/level identifiers have no authored coefficient and
    /// therefore produce no value rather than aliasing another row.
    #[must_use]
    pub fn chance_from_agility(&self, class_id: u8, level: u8, agility: i32) -> Option<f64> {
        let class_index = usize::from(class_id.checked_sub(1)?);
        let level_index = usize::from(level.checked_sub(1)?);
        if class_index >= CLASS_SLOT_COUNT as usize || level_index >= LEVEL_SLOT_COUNT as usize {
            return None;
        }
        chance_from_stat(
            &self.melee_base_by_class,
            &self.melee_per_agility_by_class_level,
            class_index,
            level_index,
            agility,
        )
    }

    /// Computes the stock spell critical-strike percentage contributed by
    /// current Intellect for one class and level.
    #[must_use]
    pub fn spell_chance_from_intellect(
        &self,
        class_id: u8,
        level: u8,
        intellect: i32,
    ) -> Option<f64> {
        let class_index = usize::from(class_id.checked_sub(1)?);
        let level_index = usize::from(level.checked_sub(1)?);
        if class_index >= CLASS_SLOT_COUNT as usize || level_index >= LEVEL_SLOT_COUNT as usize {
            return None;
        }
        chance_from_stat(
            &self.spell_base_by_class,
            &self.spell_per_intellect_by_class_level,
            class_index,
            level_index,
            intellect,
        )
    }

    /// Computes the stock out-of-combat health regeneration contributed by
    /// current Spirit for one class and level.
    #[must_use]
    pub fn health_regen_from_spirit(&self, class_id: u8, level: u8, spirit: i32) -> Option<f64> {
        let class_index = usize::from(class_id.checked_sub(1)?);
        let level_index = usize::from(level.checked_sub(1)?);
        if class_index >= CLASS_SLOT_COUNT as usize || level_index >= LEVEL_SLOT_COUNT as usize {
            return None;
        }
        let row = class_index
            .checked_mul(LEVEL_SLOT_COUNT as usize)?
            .checked_add(level_index)?;
        let oct_rate = f64::from(*self.oct_health_regen_by_class_level.get(row)?);
        let remainder_rate = f64::from(*self.health_regen_per_spirit_by_class_level.get(row)?);
        let spirit = spirit.max(0);
        let oct_spirit = spirit.min(50);
        Some(
            f64::from(oct_spirit) * oct_rate
                + f64::from(spirit.saturating_sub(oct_spirit)) * remainder_rate,
        )
    }

    /// Computes the stock mana regeneration contributed by current Intellect
    /// and Spirit for one class and level.
    #[must_use]
    pub fn mana_regen_from_spirit(
        &self,
        class_id: u8,
        level: u8,
        intellect: i32,
        spirit: i32,
    ) -> Option<f64> {
        let class_index = usize::from(class_id.checked_sub(1)?);
        let level_index = usize::from(level.checked_sub(1)?);
        if class_index >= CLASS_SLOT_COUNT as usize || level_index >= LEVEL_SLOT_COUNT as usize {
            return None;
        }
        let row = class_index
            .checked_mul(LEVEL_SLOT_COUNT as usize)?
            .checked_add(level_index)?;
        let rate = f64::from(*self.mana_regen_per_spirit_by_class_level.get(row)?);
        Some(f64::from(intellect.max(0)).sqrt() * rate * f64::from(spirit.max(0)) + 0.001)
    }
}

fn chance_from_stat(
    base_by_class: &[f32],
    per_stat_by_class_level: &[f32],
    class_index: usize,
    level_index: usize,
    stat: i32,
) -> Option<f64> {
    let base = f64::from(*base_by_class.get(class_index)?);
    let row = class_index
        .checked_mul(LEVEL_SLOT_COUNT as usize)?
        .checked_add(level_index)?;
    let per_stat = f64::from(*per_stat_by_class_level.get(row)?);
    if per_stat == 0.0 {
        return Some(0.0);
    }
    Some((per_stat * f64::from(stat.max(0)) + base) * 100.0)
}

fn require_layout(table: &WdbcTable, records: u32, name: &str) -> Result<(), AssetError> {
    let header = table.header();
    if header.record_count() == records && header.field_count() == 1 && header.record_size() == 4 {
        return Ok(());
    }
    Err(database_error(
        table,
        format!(
            "build-12340 {name} requires {records} one-float records; found {} records, {} fields, and {}-byte records",
            header.record_count(),
            header.field_count(),
            header.record_size()
        ),
    ))
}

fn float_records(table: &WdbcTable) -> Result<Vec<f32>, AssetError> {
    (0..table.header().record_count())
        .map(|row| {
            table
                .field_u32(row, 0)
                .map(f32::from_bits)
                .ok_or_else(|| database_error(table, format!("record {row} is truncated")))
        })
        .collect()
}
