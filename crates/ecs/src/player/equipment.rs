//! Public player visible-item fields retained for character presentation.

use shipyard::Component;

/// Number of public equipment slots in the build-12340 update table.
pub const PLAYER_EQUIPMENT_SLOT_COUNT: usize = 19;

/// One of the nineteen public player visible-item slots in server order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PlayerEquipmentSlot {
    /// Head.
    Head = 0,
    /// Neck.
    Neck = 1,
    /// Shoulders.
    Shoulders = 2,
    /// Shirt.
    Shirt = 3,
    /// Chest.
    Chest = 4,
    /// Waist.
    Waist = 5,
    /// Legs.
    Legs = 6,
    /// Feet.
    Feet = 7,
    /// Wrists.
    Wrists = 8,
    /// Hands.
    Hands = 9,
    /// First finger.
    FingerOne = 10,
    /// Second finger.
    FingerTwo = 11,
    /// First trinket.
    TrinketOne = 12,
    /// Second trinket.
    TrinketTwo = 13,
    /// Back.
    Back = 14,
    /// Main hand.
    MainHand = 15,
    /// Off hand.
    OffHand = 16,
    /// Ranged or relic.
    Ranged = 17,
    /// Tabard.
    Tabard = 18,
}

impl PlayerEquipmentSlot {
    /// All public slots in update-field order.
    pub const ALL: [Self; PLAYER_EQUIPMENT_SLOT_COUNT] = [
        Self::Head,
        Self::Neck,
        Self::Shoulders,
        Self::Shirt,
        Self::Chest,
        Self::Waist,
        Self::Legs,
        Self::Feet,
        Self::Wrists,
        Self::Hands,
        Self::FingerOne,
        Self::FingerTwo,
        Self::TrinketOne,
        Self::TrinketTwo,
        Self::Back,
        Self::MainHand,
        Self::OffHand,
        Self::Ranged,
        Self::Tabard,
    ];

    /// Returns the zero-based server slot index.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Public presentation words for one equipped item.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VisibleEquipmentItem {
    entry_id: u32,
    enchantment_word: u32,
}

impl VisibleEquipmentItem {
    /// Creates one exact visible-item field pair.
    #[must_use]
    pub const fn new(entry_id: u32, enchantment_word: u32) -> Self {
        Self {
            entry_id,
            enchantment_word,
        }
    }

    /// Returns the `Item.dbc` entry identifier, or zero for an empty slot.
    #[must_use]
    pub const fn entry_id(self) -> u32 {
        self.entry_id
    }

    /// Returns the packed public enchantment word without discarding either half.
    #[must_use]
    pub const fn enchantment_word(self) -> u32 {
        self.enchantment_word
    }

    /// Returns the permanent enchantment identifier stored in the low half.
    #[must_use]
    pub const fn permanent_enchantment_id(self) -> u16 {
        self.enchantment_word as u16
    }

    /// Returns the temporary enchantment identifier stored in the high half.
    #[must_use]
    pub const fn temporary_enchantment_id(self) -> u16 {
        (self.enchantment_word >> 16) as u16
    }
}

/// Snapshot of all public equipment fields for one player entity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct PlayerEquipment {
    items: [VisibleEquipmentItem; PLAYER_EQUIPMENT_SLOT_COUNT],
}

impl PlayerEquipment {
    /// Creates a snapshot in exact public slot order.
    #[must_use]
    pub const fn new(items: [VisibleEquipmentItem; PLAYER_EQUIPMENT_SLOT_COUNT]) -> Self {
        Self { items }
    }

    /// Returns one visible slot.
    #[must_use]
    pub const fn item(self, slot: PlayerEquipmentSlot) -> VisibleEquipmentItem {
        self.items[slot.index()]
    }

    /// Returns every visible slot in server order.
    #[must_use]
    pub const fn items(self) -> [VisibleEquipmentItem; PLAYER_EQUIPMENT_SLOT_COUNT] {
        self.items
    }
}
