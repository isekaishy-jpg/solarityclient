//! Screen-effect callbacks within native aura visual admission.

use solarity_asset::SpellEffectDefinition;
use solarity_ecs::UnitAura;

#[derive(Default)]
pub(super) struct AuraCallbacks {
    /// The native visual bank is separate from raw server aura slots.
    pub(super) visual_spells: Vec<u32>,
}

impl AuraCallbacks {
    /// 72F5D0 installs all wire slots, then tears down and admits visuals in
    /// ascending slot order. Every callback sees that same final raw aura bank.
    pub(super) fn receive(
        &mut self,
        previous: &[UnitAura],
        current: &[UnitAura],
        touched: &[bool; 256],
        replace: bool,
        player_bytes: u32,
        spell: impl Fn(u32) -> Option<SpellEffectDefinition>,
    ) -> usize {
        self.visual_spells
            .resize(current.len().max(previous.len()), 0);
        let mut callbacks = 0;
        for (index, old) in previous.iter().enumerate().filter(|(i, _)| touched[*i]) {
            let new = current.get(index).copied().unwrap_or_default();
            let old_active = old.spell != 0 && old.flags & 7 != 0;
            let retained = old.spell == new.spell && new.flags & 7 != 0;
            if old_active && !retained || new.spell == 0 {
                let definition = spell(old.spell);
                if !replace || definition.is_some() {
                    callbacks += screen_callbacks(definition.unwrap_or_default());
                    self.visual_spells[index] = 0;
                }
            }
        }
        for (index, new) in current.iter().enumerate().filter(|(i, _)| touched[*i]) {
            let old = previous.get(index).copied().unwrap_or_default();
            if new.spell == 0
                || new.flags & 7 == 0
                || (!replace && old.spell == new.spell && old.flags & 7 != 0)
            {
                continue;
            }
            let definition = spell(new.spell);
            // Partial updates pass a zero-filled record even on a Spell miss;
            // full updates skip that visual admission entirely.
            if replace && definition.is_none() {
                continue;
            }
            callbacks += self.admit(index, new.spell, definition, player_bytes, &spell);
        }
        callbacks
    }

    /// 727A70 visits every raw slot, irrespective of its effect flags. The
    /// visitor tests the low byte of the mask before calling visual admission.
    pub(super) fn vision_changed(
        &mut self,
        current: &[UnitAura],
        changed: u8,
        player_bytes: u32,
        spell: impl Fn(u32) -> Option<SpellEffectDefinition>,
    ) -> usize {
        self.visual_spells.resize(current.len(), 0);
        let mut callbacks = 0;
        for (index, aura) in current.iter().enumerate() {
            let Some(definition) = spell(aura.spell) else {
                continue;
            };
            let mask = aura_vision_mask(definition) as u8;
            if changed & mask == 0 {
                continue;
            }
            if (player_bytes >> 24) as u8 & mask == 0 {
                callbacks += screen_callbacks(definition);
                self.visual_spells[index] = 0;
            } else {
                callbacks += self.admit(index, aura.spell, Some(definition), player_bytes, &spell);
            }
        }
        callbacks
    }

    fn admit(
        &mut self,
        index: usize,
        id: u32,
        definition: Option<SpellEffectDefinition>,
        player_bytes: u32,
        spell: &impl Fn(u32) -> Option<SpellEffectDefinition>,
    ) -> usize {
        let record_id = definition.map_or(0, |_| id);
        let definition = definition.unwrap_or_default();
        let visual = self.visual_spells[index];
        if visual != 0
            && (visual == record_id
                || spell(visual)
                    .is_some_and(|old| definition.visual_priority < old.visual_priority))
        {
            return 0;
        }
        let mask = aura_vision_mask(definition);
        if mask != 0 && mask & (player_bytes >> 24) == 0 {
            return 0;
        }
        self.visual_spells[index] = id;
        screen_callbacks(definition)
    }
}

fn screen_callbacks(spell: SpellEffectDefinition) -> usize {
    spell.aura_types.iter().filter(|&&kind| kind == 260).count()
}

/// 7FE3E0 retains the 32-bit shift, while its consumer admits only byte bits.
fn aura_vision_mask(spell: SpellEffectDefinition) -> u32 {
    let mut mask = if spell.required_aura_vision < 1 {
        0
    } else {
        1 << ((spell.required_aura_vision as u32).wrapping_sub(1) & 31)
    };
    for (kind, value) in spell.aura_types.into_iter().zip(spell.misc_values) {
        if kind == 17 {
            mask |= 0x20;
        } else if kind == 19 && matches!(value, 0 | 10) {
            mask |= 0x40;
        }
    }
    mask
}

#[cfg(test)]
#[path = "../../../../tests/application/screen_effect_callbacks.rs"]
mod tests;
