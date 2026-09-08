//! Retained Unit_C aura slots, independent of spell resource residency.

/// The native 24-byte aura record represented without its padding byte.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnitAura {
    /// Zero denotes an inactive slot.
    pub spell: u32,
    /// Enabled effect bits and the native aura flags.
    pub flags: u8,
    /// Transmitted caster level.
    pub level: u8,
    /// Transmitted application count.
    pub applications: u8,
    /// Retained caster GUID, even when that unit is not resident.
    pub caster: u64,
    /// Maximum authored milliseconds, zero when duration is absent.
    pub duration_ms: u32,
    /// Wrapping receipt plus remaining duration, with zero replaced by one.
    pub end_ms: u32,
}

/// Sparse growth preserves the complete unsigned-byte slot namespace.
#[derive(Clone, Debug, Default, Eq, PartialEq, shipyard::Component)]
pub struct UnitAuras {
    slots: Vec<UnitAura>,
}

impl UnitAuras {
    /// Returns retained slots, including inactive records.
    #[must_use]
    pub fn slots(&self) -> &[UnitAura] {
        &self.slots
    }
    /// Returns a zero record for a slot never received.
    #[must_use]
    pub fn slot(&self, slot: u8) -> UnitAura {
        self.slots
            .get(usize::from(slot))
            .copied()
            .unwrap_or_default()
    }
    /// Full replacements zero the old records before their new slot updates.
    pub fn clear(&mut self) {
        self.slots.fill(UnitAura::default());
    }
    /// Replaces one active slot, or only its spell word for native removal.
    pub fn set(&mut self, slot: u8, aura: UnitAura) {
        let index = usize::from(slot);
        if self.slots.len() <= index {
            self.slots.resize(index + 1, UnitAura::default());
        }
        if aura.spell == 0 {
            self.slots[index].spell = 0;
        } else {
            self.slots[index] = aura;
        }
    }
}
