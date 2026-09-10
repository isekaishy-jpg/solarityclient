//! Local player screen-effect selection, before authored effect dispatch.

use solarity_asset::SpellEffectDefinition;
use solarity_ecs::{ObjectFields, UnitAura, UnitAuras};

impl super::RuntimeGameplayCoordinator {
    /// 4F88B0 checks descending aura slots before ghost and invisibility flags.
    pub(in crate::application) fn screen_effect_id(&self) -> u32 {
        let Some(world) = self.world() else {
            return 0;
        };
        let Ok(fields) = world.storage().get::<&ObjectFields>(world.local_player()) else {
            return 0;
        };
        let auras = world.storage().get::<&UnitAuras>(world.local_player()).ok();
        select(
            auras.as_ref().map_or(&[], |auras| auras.slots()),
            fields.get(150),
            fields.get(1229),
            self.player_ui.arena,
            |id| self.spells.as_ref()?.spell(id).copied(),
        )
    }
}

/// Aura flags do not mask the authored effect triplet in this native owner.
fn select(
    auras: &[UnitAura],
    player_flags: u32,
    player_bytes_2: u32,
    arena: bool,
    mut spell: impl FnMut(u32) -> Option<SpellEffectDefinition>,
) -> u32 {
    for aura in auras.iter().rev().filter(|aura| aura.spell != 0) {
        if let Some(spell) = spell(aura.spell)
            && let Some(index) = spell.aura_types.iter().position(|&kind| kind == 260)
        {
            return spell.misc_values[index];
        }
    }
    if player_flags & 0x10 != 0 && !arena {
        1
    } else if player_bytes_2 & 0x40000000 != 0 {
        81
    } else {
        0
    }
}

#[cfg(test)]
#[path = "../../../tests/application/screen_effect.rs"]
mod tests;
