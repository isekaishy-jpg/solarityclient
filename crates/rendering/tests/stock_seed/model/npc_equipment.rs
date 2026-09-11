//! Native Unit_C virtual-item selection, including disarms and body readiness.

use super::*;
use solarity_ecs::UnitFlags;
use solarity_rendering::{NpcWeaponAnimationInput, NpcWeaponState};

#[test]
fn npc_weapon_state_matches_native_reconciliation() -> Result<(), Box<dyn Error>> {
    let native = include_str!("../../fixtures/npc_weapon_state_native.txt");
    let rows = native
        .lines()
        .filter_map(|line| line.strip_prefix("item "))
        .flat_map(str::split_whitespace)
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()?;
    let bytes = create_wdbc((rows.len() / 8) as u32, 8, &rows, b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        path: "DBFilesClient\\Item.dbc",
        bytes: &bytes,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let definitions = ItemDefinitionCatalog::load(&mut AssetStore::mount(catalog)?)?;
    let mut count = 0;
    for line in native.lines().filter_map(|line| line.strip_prefix("case ")) {
        let (inputs, expected) = line.split_once(';').ok_or("native delimiter")?;
        let values = inputs
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let sheath = |value| UnitSheathState::try_from(value as u8).map_err(|_| "sheath");
        let actual = NpcWeaponState::new(
            sheath(values[1])?,
            UnitFlags::new(values[7], values[8], 0),
            0,
            values[3] as u16,
        )
        .reconcile(
            sheath(values[0])?,
            NpcWeaponAnimationInput {
                animation_id: (values[2] != u32::MAX).then_some(values[2]),
                weapon_flags: values[4],
                has_attack_target: values[5] != 0,
                template_flags: values[6],
                changed_stand_state: None,
            },
            [definitions.item(values[9]), definitions.item(values[10])],
        );
        assert_eq!(
            actual.sheath_state(),
            sheath(expected.parse::<u32>()?)?,
            "native inputs {inputs}"
        );
        count += 1;
    }
    assert_eq!(count, 10368);
    Ok(())
}

#[test]
fn npc_virtual_items_match_native_selection() -> Result<(), Box<dyn Error>> {
    let native = include_str!("../../fixtures/npc_virtual_items_native.txt");
    let mut rows = Vec::new();
    for line in native.lines().filter_map(|line| line.strip_prefix("item ")) {
        rows.extend(
            line.split_whitespace()
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let definitions_bytes = create_wdbc((rows.len() / 8) as u32, 8, &rows, b"\0");
    let display_rows: Vec<_> = rows
        .as_chunks::<8>()
        .0
        .iter()
        .flat_map(|row| item_display_model_fields(row[5], 1, 11, 700 + row[0], 900 + row[0]))
        .collect();
    let display_bytes = create_wdbc(
        (rows.len() / 8) as u32,
        25,
        &display_rows,
        b"\0Weapon.m2\0Texture\0",
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &definitions_bytes,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &display_bytes,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let slots = [
        PlayerEquipmentSlot::MainHand,
        PlayerEquipmentSlot::OffHand,
        PlayerEquipmentSlot::Ranged,
    ];
    let mut count = 0;
    for line in native.lines().filter_map(|line| line.strip_prefix("case ")) {
        let (inputs, expected) = line.split_once(';').ok_or("native delimiter")?;
        let values = inputs
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let equipment = std::array::from_fn(|index| {
            let definition = definitions.item(values[5 + index])?;
            let display = displays.display(definition.display_info_id())?;
            Some(CharacterEquipmentItem::new_visible(
                slots[index],
                VisibleEquipmentItem::new(values[5 + index], 0),
                definition,
                display,
            ))
        });
        let mut plan = CharacterAttachmentPlan::default();
        plan.add_npc_held_items(
            equipment,
            NpcWeaponState::new(
                UnitSheathState::try_from(values[2] as u8).map_err(|_| "sheath")?,
                UnitFlags::new(values[0], values[1], 0),
                values[4],
                values[3] as u16,
            ),
            [definitions.item(values[5]), definitions.item(values[6])],
        )?;
        let actual = plan
            .attachments()
            .iter()
            .map(|attachment| {
                let slot = slots
                    .iter()
                    .position(|slot| Some(*slot) == attachment.slot())
                    .ok_or("slot")?;
                let definition = definitions.item(values[5 + slot]).ok_or("definition")?;
                let display = displays
                    .display(definition.display_info_id())
                    .ok_or("display")?;
                let folder = if slot == 1 && definition.inventory_type() == InventoryType::Shield {
                    "SHIELD"
                } else {
                    "WEAPON"
                };
                assert_eq!(
                    attachment.model().as_str(),
                    format!("ITEM\\OBJECTCOMPONENTS\\{folder}\\WEAPON.M2")
                );
                assert_eq!(
                    attachment.texture().map(AssetPath::as_str),
                    Some(format!("ITEM\\OBJECTCOMPONENTS\\{folder}\\TEXTURE.BLP").as_str())
                );
                assert_eq!(attachment.item_visual_id(), display.item_visual_id());
                assert_eq!(attachment.particle_color_id(), display.particle_color_id());
                assert_eq!(attachment.enchantment_word(), 0);
                Ok(format!("{slot}:{}", attachment.point().id()))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?
            .join(" ");
        assert_eq!(actual, expected, "native inputs {inputs}");
        count += 1;
    }
    assert_eq!(count, 1536);
    // A missing main-hand display still suppresses the off-hand component when
    // the retained Item.dbc metadata describes a two-handed weapon.
    let main = definitions.item(104).ok_or("two-handed definition")?;
    let off = definitions.item(101).ok_or("shield definition")?;
    let off_display = displays
        .display(off.display_info_id())
        .ok_or("shield display")?;
    let mut missing_main_display = CharacterAttachmentPlan::default();
    missing_main_display.add_npc_held_items(
        [
            None,
            Some(CharacterEquipmentItem::new_visible(
                slots[1],
                VisibleEquipmentItem::new(101, 0),
                off,
                off_display,
            )),
            None,
        ],
        NpcWeaponState::new(UnitSheathState::Melee, UnitFlags::default(), 0, 0),
        [Some(main), Some(off)],
    )?;
    assert!(missing_main_display.attachments().is_empty());
    Ok(())
}
