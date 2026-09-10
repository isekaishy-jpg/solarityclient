//! Local player screen-effect selection, before authored effect dispatch.

mod auras;

use solarity_asset::SpellEffectDefinition;
use solarity_ecs::{ActiveWorld, ObjectFields, UnitAura, UnitAuras, WorldObjectIdentity};
use std::collections::VecDeque;

impl super::RuntimeGameplayCoordinator {
    /// Returns the selection retained by the most recent player-effect callback.
    #[must_use]
    pub fn screen_effect_id(&self) -> u32 {
        self.player_ui.screen_effect.current
    }

    pub(in crate::application) fn take_screen_effect_update(&mut self) -> Option<u32> {
        self.player_ui.screen_effect.pending.pop_front()
    }
}

/// The effect owner changes on native callbacks, independently of render frames.
pub(super) struct RuntimePlayerScreenEffects {
    pub(super) current: u32,
    pending: VecDeque<u32>,
    owner: Option<WorldObjectIdentity>,
    initialized: bool,
    previous_auras: Vec<UnitAura>,
    touched: [bool; 256],
    replace: bool,
    callbacks: auras::AuraCallbacks,
}

impl Default for RuntimePlayerScreenEffects {
    fn default() -> Self {
        Self {
            current: 0,
            pending: VecDeque::new(),
            owner: None,
            initialized: false,
            previous_auras: Vec::new(),
            touched: [false; 256],
            replace: false,
            callbacks: auras::AuraCallbacks::default(),
        }
    }
}

impl RuntimePlayerScreenEffects {
    pub(super) fn reset(&mut self) {
        self.current = 0;
        self.pending.clear();
        self.pending.push_back(0);
        self.owner = None;
        self.initialized = false;
        self.previous_auras.clear();
        self.callbacks.visual_spells.clear();
    }

    fn synchronize_owner(&mut self, world: &ActiveWorld) {
        let owner = world
            .local_player_guid()
            .ok()
            .and_then(|guid| world.object_identity(guid));
        if self.owner != owner {
            if self.owner.is_some() {
                self.reset();
            }
            self.owner = owner;
        }
    }

    pub(super) fn initialize(
        &mut self,
        world: &ActiveWorld,
        spells: Option<&solarity_asset::SpellEffectCatalog>,
        arena: bool,
    ) {
        self.synchronize_owner(world);
        if !self.initialized {
            self.initialized = true;
            self.refresh(world, spells, arena);
        }
    }

    pub(super) fn refresh(
        &mut self,
        world: &ActiveWorld,
        spells: Option<&solarity_asset::SpellEffectCatalog>,
        arena: bool,
    ) {
        self.synchronize_owner(world);
        self.current = Self::select_world(world, spells, arena);
        self.pending.push_back(self.current);
    }

    fn select_world(
        world: &ActiveWorld,
        spells: Option<&solarity_asset::SpellEffectCatalog>,
        arena: bool,
    ) -> u32 {
        let Ok(fields) = world.storage().get::<&ObjectFields>(world.local_player()) else {
            return 0;
        };
        let auras = world.storage().get::<&UnitAuras>(world.local_player()).ok();
        select(
            auras.as_ref().map_or(&[], |auras| auras.slots()),
            fields.get(150),
            fields.get(1229),
            arena,
            |id| spells?.spell(id).copied(),
        )
    }

    pub(super) fn before_auras(
        &mut self,
        world: &ActiveWorld,
        update: &solarity_network::WorldUnitAuraUpdate,
    ) {
        self.synchronize_owner(world);
        self.previous_auras.clear();
        if let Ok(auras) = world.storage().get::<&UnitAuras>(world.local_player()) {
            self.previous_auras.extend_from_slice(auras.slots());
        }
        self.replace = update.replace;
        self.touched.fill(update.replace);
        for aura in &update.auras {
            self.touched[aura.slot as usize] = true;
        }
    }

    pub(super) fn vision_changed(
        &mut self,
        world: &ActiveWorld,
        spells: Option<&solarity_asset::SpellEffectCatalog>,
        arena: bool,
        changed: u8,
    ) {
        self.synchronize_owner(world);
        // 6DA770 refreshes first, then visits units using the changed player's
        // mask and the local player's current visibility byte.
        if changed & 0x40 != 0 {
            self.refresh(world, spells, arena);
        }
        let Ok(auras) = world.storage().get::<&UnitAuras>(world.local_player()) else {
            return;
        };
        let bytes = world
            .storage()
            .get::<&ObjectFields>(world.local_player())
            .map_or(0, |f| f.get(1229));
        let count = self
            .callbacks
            .vision_changed(auras.slots(), changed, bytes, |id| {
                spells?.spell(id).copied()
            });
        if count != 0 {
            self.current = Self::select_world(world, spells, arena);
            self.pending
                .extend(std::iter::repeat_n(self.current, count));
        }
    }

    pub(super) fn after_auras(
        &mut self,
        world: &ActiveWorld,
        spells: Option<&solarity_asset::SpellEffectCatalog>,
        arena: bool,
    ) {
        let Ok(auras) = world.storage().get::<&UnitAuras>(world.local_player()) else {
            return;
        };
        let bytes = world
            .storage()
            .get::<&ObjectFields>(world.local_player())
            .map_or(0, |f| f.get(1229));
        let count = self.callbacks.receive(
            &self.previous_auras,
            auras.slots(),
            &self.touched,
            self.replace,
            bytes,
            |id| spells?.spell(id).copied(),
        );
        if count != 0 {
            self.current = Self::select_world(world, spells, arena);
            self.pending
                .extend(std::iter::repeat_n(self.current, count));
        }
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
