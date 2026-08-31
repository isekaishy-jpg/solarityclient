//! External stock-compatibility tests for `ecs/player` belong here.

use solarity_ecs::VisibleEquipmentItem;

/// Public visible-item enchantments retain their two independently updated halves.
#[test]
fn visible_equipment_item_exposes_permanent_and_temporary_enchantments() {
    let item = VisibleEquipmentItem::new(19_050, 0xABCD_1234);

    assert_eq!(item.entry_id(), 19_050);
    assert_eq!(item.enchantment_word(), 0xABCD_1234);
    assert_eq!(item.permanent_enchantment_id(), 0x1234);
    assert_eq!(item.temporary_enchantment_id(), 0xABCD);
}
