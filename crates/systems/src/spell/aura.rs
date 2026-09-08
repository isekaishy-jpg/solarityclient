//! Active effect-mask projection from native Unit_C `727E70`.

use solarity_asset::SpellEffectCatalog;
use solarity_ecs::UnitAuras;

/// Queries enabled aura effects without expiring authoritative slots locally.
#[must_use]
pub fn unit_has_aura_type(auras: &UnitAuras, spells: &SpellEffectCatalog, kind: u32) -> bool {
    auras
        .slots()
        .iter()
        .filter(|aura| aura.spell != 0)
        .any(|aura| {
            spells.spell(aura.spell).is_some_and(|spell| {
                spell
                    .aura_types
                    .iter()
                    .enumerate()
                    .any(|(index, value)| aura.flags & (1 << index) != 0 && *value == kind)
            })
        })
}
