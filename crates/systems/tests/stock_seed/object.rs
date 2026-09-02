//! External stock-compatibility tests for object update projection.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, GameObjectPresentation, ObjectFields, ObjectKind, ObjectPresentation,
    PlayerAppearance, PlayerEquipment, PlayerEquipmentSlot, PlayerMoney, PlayerProgression,
    UnitAnimationTier, UnitFlags, UnitIdentity, UnitPresentation, UnitSheathState, UnitVitals,
    WorldBootstrap, WorldMapId,
};
use solarity_systems::{ObjectProjectionError, project_object_fields};

/// Build-12340 player words project into typed views and preserve sparse state.
#[test]
fn player_update_fields_project_without_losing_sparse_values() -> Result<(), Box<dyn Error>> {
    let guid = 0x0000_0000_0000_0042;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        guid,
        "Solarion",
        Vec3::ZERO,
        0.0,
    ));
    let create_fields = [
        (3, 0),
        (4, 1.0_f32.to_bits()),
        (23, u32::from_le_bytes([1, 8, 0, 0])),
        (24, 1_000),
        (25, 4_000),
        (32, 1_500),
        (33, 5_000),
        (54, 80),
        (55, 1),
        (59, 0x0000_0008),
        (60, 0x0000_0800),
        (67, 20_000),
        (68, 20_001),
        (69, 14_307),
        (74, u32::from_le_bytes([1, 0, 0, 3])),
        (79, 0x0000_0001),
        (122, u32::from_le_bytes([1, 0x20, 0, 0])),
        (153, u32::from_le_bytes([3, 4, 5, 6])),
        (154, u32::from_le_bytes([7, 0, 0, 2])),
        (283, 50_001),
        (284, u32::from_le_bytes([17, 0, 23, 0])),
        (319, 50_019),
        (320, u32::from_le_bytes([31, 0, 0, 0])),
        (0x027A, 123_456),
        (0x027B, 1_000_000),
        (0x0492, 12_345_678),
    ];
    world.create_object(guid, ObjectKind::Player, None, create_fields)?;
    project_object_fields(&mut world, guid, create_fields)?;

    let player = world.local_player();
    let object = *world.storage().get::<&ObjectPresentation>(player)?;
    assert_eq!(object.entry_id(), 0);
    assert_eq!(object.scale(), 1.0);

    let identity = **world.storage().get::<&UnitIdentity>(player)?;
    assert_eq!(identity.race_id(), 1);
    assert_eq!(identity.class_id(), 8);
    assert_eq!(identity.gender_id(), 0);
    assert_eq!(identity.power_type_id(), 0);
    assert_eq!(identity.level(), 80);
    assert_eq!(identity.faction_template_id(), 1);

    let vitals = *world.storage().get::<&UnitVitals>(player)?;
    assert_eq!(vitals.health(), 1_000);
    assert_eq!(vitals.max_health(), 1_500);
    assert_eq!(vitals.powers()[0], 4_000);
    assert_eq!(vitals.max_powers()[0], 5_000);

    let presentation = *world.storage().get::<&UnitPresentation>(player)?;
    assert_eq!(presentation.display_id(), 20_000);
    assert_eq!(presentation.native_display_id(), 20_001);
    assert_eq!(presentation.mount_display_id(), 14_307);
    assert_eq!(presentation.stand_state(), 1);
    assert_eq!(presentation.animation_tier(), UnitAnimationTier::Fly);
    assert_eq!(presentation.sheath_state(), UnitSheathState::Melee);

    let flags = *world.storage().get::<&UnitFlags>(player)?;
    assert_eq!(flags.primary(), 0x0000_0008);
    assert_eq!(flags.secondary(), 0x0000_0800);
    assert_eq!(flags.dynamic(), 0x0000_0001);

    let appearance = *world.storage().get::<&PlayerAppearance>(player)?;
    assert_eq!(appearance.skin_id(), 3);
    assert_eq!(appearance.face_id(), 4);
    assert_eq!(appearance.hair_style_id(), 5);
    assert_eq!(appearance.hair_color_id(), 6);
    assert_eq!(appearance.facial_hair_style_id(), 7);

    let equipment = *world.storage().get::<&PlayerEquipment>(player)?;
    assert_eq!(equipment.item(PlayerEquipmentSlot::Head).entry_id(), 50_001);
    assert_eq!(
        equipment.item(PlayerEquipmentSlot::Head).enchantment_word(),
        u32::from_le_bytes([17, 0, 23, 0])
    );
    assert_eq!(
        equipment.item(PlayerEquipmentSlot::Tabard).entry_id(),
        50_019
    );
    assert_eq!(
        world.local_player_money(),
        Some(PlayerMoney::new(12_345_678))
    );
    assert_eq!(
        world.local_player_progression(),
        Some(PlayerProgression::new(123_456, 1_000_000))
    );
    assert_eq!(world.local_player_unit_identity(), Some(identity));

    // A VALUES update carries only changed words; all other typed values must
    // remain intact just as they do in the authoritative dense table.
    let values_fields = [
        (24, 750),
        (154, u32::from_le_bytes([9, 0, 0, 2])),
        (283, 50_101),
        (0x027A, 234_567),
        (0x0492, u32::MAX),
    ];
    world.update_fields(guid, values_fields)?;
    project_object_fields(&mut world, guid, values_fields)?;
    let vitals = *world.storage().get::<&UnitVitals>(player)?;
    assert_eq!(vitals.health(), 750);
    assert_eq!(vitals.max_health(), 1_500);
    let appearance = *world.storage().get::<&PlayerAppearance>(player)?;
    assert_eq!(appearance.skin_id(), 3);
    assert_eq!(appearance.facial_hair_style_id(), 9);
    let equipment = *world.storage().get::<&PlayerEquipment>(player)?;
    assert_eq!(equipment.item(PlayerEquipmentSlot::Head).entry_id(), 50_101);
    assert_eq!(
        equipment.item(PlayerEquipmentSlot::Head).enchantment_word(),
        u32::from_le_bytes([17, 0, 23, 0])
    );
    assert_eq!(world.local_player_money(), Some(PlayerMoney::new(u32::MAX)));
    assert_eq!(
        world.local_player_progression(),
        Some(PlayerProgression::new(234_567, 1_000_000))
    );
    assert_eq!(world.storage().get::<&ObjectFields>(player)?.get(24), 750);
    Ok(())
}

/// A create mask omits zero words while the stock dense player table still
/// makes those private values authoritative to synchronous FrameXML queries.
#[test]
fn local_player_create_materializes_omitted_zero_progression_and_money()
-> Result<(), Box<dyn Error>> {
    let guid = 0x0000_0000_0000_0042;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(1),
        guid,
        "MaxLevel",
        Vec3::ZERO,
        0.0,
    ));
    let create_fields = [
        (4, 1.0_f32.to_bits()),
        (23, u32::from_le_bytes([1, 1, 0, 0])),
        (24, 1_000),
        (32, 1_000),
        (54, 80),
        (55, 1),
        (67, 49),
        (68, 49),
    ];
    world.create_object(guid, ObjectKind::Player, None, create_fields)?;
    project_object_fields(&mut world, guid, create_fields)?;

    assert_eq!(world.local_player_money(), Some(PlayerMoney::new(0)));
    assert_eq!(
        world.local_player_progression(),
        Some(PlayerProgression::new(0, 0))
    );

    // An unrelated sparse values update must retain both zero-valued views.
    let values_fields = [(24, 999)];
    world.update_fields(guid, values_fields)?;
    project_object_fields(&mut world, guid, values_fields)?;
    assert_eq!(world.local_player_money(), Some(PlayerMoney::new(0)));
    assert_eq!(
        world.local_player_progression(),
        Some(PlayerProgression::new(0, 0))
    );
    Ok(())
}

/// Game-object display identity survives sparse field projection for residency.
#[test]
fn game_object_display_field_projects_without_unit_aliasing() -> Result<(), Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    let guid = 0xF110_0000_0000_002A;
    let create_fields = [
        (3, 17),
        (4, 1.0_f32.to_bits()),
        (8, 42),
        (17, u32::from_le_bytes([1, 5, 7, 100])),
    ];
    world.create_object(
        guid,
        ObjectKind::GameObject,
        Some(solarity_ecs::WorldTransform::new(Vec3::ZERO, 0.0)),
        create_fields,
    )?;
    project_object_fields(&mut world, guid, create_fields)?;

    let presentation = world
        .game_object_presentation(guid)
        .ok_or("game-object presentation was absent")?;
    assert_eq!(presentation, GameObjectPresentation::new(42, 1));
    assert!(world.game_object_presentation(guid).is_some());

    let sparse_fields = [(9, 0x20)];
    world.update_fields(guid, sparse_fields)?;
    project_object_fields(&mut world, guid, sparse_fields)?;
    assert_eq!(
        world
            .game_object_presentation(guid)
            .ok_or("sparse update discarded game-object presentation")?
            .display_id(),
        42
    );
    assert_eq!(
        world
            .game_object_presentation(guid)
            .ok_or("sparse update discarded game-object state")?
            .state(),
        1
    );
    assert!(world.unit_presentation(guid).is_none());
    Ok(())
}

/// Projection rejects a pre-seeded player until its create type arrives.
#[test]
fn projection_requires_the_stock_create_type() -> Result<(), Box<dyn Error>> {
    let guid = 0x42;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        guid,
        "Solarion",
        Vec3::ZERO,
        0.0,
    ));

    assert_eq!(
        project_object_fields(&mut world, guid, [(24, 1)]),
        Err(ObjectProjectionError::MissingObjectKind { guid })
    );
    assert_eq!(
        project_object_fields(&mut world, 0x99, [(24, 1)]),
        Err(ObjectProjectionError::UnknownObject { guid: 0x99 })
    );
    world.create_object(guid, ObjectKind::Player, None, [(122, 3)])?;
    assert_eq!(
        project_object_fields(&mut world, guid, [(122, 3)]),
        Err(ObjectProjectionError::InvalidSheathState { guid, state: 3 })
    );
    assert_eq!(
        project_object_fields(&mut world, guid, [(74, u32::from_le_bytes([0, 0, 0, 5]))]),
        Err(ObjectProjectionError::InvalidAnimationTier { guid, tier: 5 })
    );
    Ok(())
}
