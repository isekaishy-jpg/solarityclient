//! Stock implementation responsibility recovered from `Object_C.cpp`.

use shipyard::Component;

/// Stable server GUID attached to every network-created world object.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Component)]
pub struct ObjectGuid(u64);

impl ObjectGuid {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the exact 64-bit world object GUID.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Build-12340 object category attached at create time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component)]
pub enum ObjectKind {
    /// Base object.
    Object,
    /// Inventory item.
    Item,
    /// Inventory container.
    Container,
    /// Creature or unit.
    Unit,
    /// Player.
    Player,
    /// Static or interactive game object.
    GameObject,
    /// Dynamic spell object.
    DynamicObject,
    /// Corpse.
    Corpse,
}

/// Dense build-12340 update-field storage indexed exactly like the client table.
#[derive(Clone, Debug, Default, Eq, PartialEq, Component)]
pub struct ObjectFields {
    values: Vec<u32>,
}

impl ObjectFields {
    pub(crate) fn apply(&mut self, fields: impl IntoIterator<Item = (u16, u32)>) {
        for (index, value) in fields {
            let index = usize::from(index);
            if index >= self.values.len() {
                self.values.resize(index + 1, 0);
            }
            self.values[index] = value;
        }
    }

    /// Returns a field word, or zero for an unset in-range or future field.
    #[must_use]
    pub fn get(&self, index: u16) -> u32 {
        self.values.get(usize::from(index)).copied().unwrap_or(0)
    }

    /// Returns the allocated field-table extent.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether no update field has been materialized.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
