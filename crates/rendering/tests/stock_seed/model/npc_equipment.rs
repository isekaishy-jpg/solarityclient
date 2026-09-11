//! Native Unit_C virtual-item selection, including disarms and body readiness.

use super::*;
use solarity_ecs::UnitFlags;
use solarity_rendering::NpcWeaponState;

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
    Ok(())
}
