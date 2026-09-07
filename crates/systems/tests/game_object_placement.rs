//! Native packed quaternion decoding and GameObject placement ownership.

use glam::{Vec3, Vec4};
use solarity_ecs::{
    ActiveWorld, GameObjectAnimatedPose, GameObjectMovement, GameObjectTransport, ObjectKind,
    ObjectPresentation, WorldBootstrap, WorldMapId, WorldTransform,
};
use solarity_systems::{
    GameObjectPlacement, GameObjectPlacementError, GameObjectPlacementResolver,
    unpack_game_object_rotation,
};

#[test]
fn composed_passenger_placements_match_native_operations() -> Result<(), Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "NativePassenger",
        Vec3::ZERO,
        0.0,
    ));
    let parent = world.create_object(2, ObjectKind::GameObject, None, [])?;
    let child = world.create_object(3, ObjectKind::GameObject, None, [])?;
    let mut resolver = GameObjectPlacementResolver::default();
    let mut count = 0;
    for line in include_str!("fixtures/game-object-passenger-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let parent_packed = u64::from_str_radix(words[0], 16)?;
        let local_packed = u64::from_str_radix(words[1], 16)?;
        let bits = words[2..]
            .iter()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let values = bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>();
        world.update_transform(
            2,
            WorldTransform::new(Vec3::new(values[0], values[1], values[2]), 0.0),
        )?;
        world
            .storage_mut()
            .add_component(parent, (ObjectPresentation::new(1, values[6]),));
        world
            .storage_mut()
            .add_component(child, (ObjectPresentation::new(1, values[7]),));
        world.update_game_object_movement(2, GameObjectMovement::new(parent_packed, None))?;
        world.update_game_object_movement(
            3,
            GameObjectMovement::new(
                local_packed,
                Some(GameObjectTransport {
                    guid: 2,
                    position: Vec3::new(values[3], values[4], values[5]),
                    orientation: 99.0,
                }),
            ),
        )?;
        let placement = resolver.resolve(&world, 3)?;
        let cached = resolver.resolve(&world, 3)?;
        assert_eq!(
            cached.matrix().to_cols_array().map(f32::to_bits),
            placement.matrix().to_cols_array().map(f32::to_bits)
        );
        assert_eq!(
            cached.rotation().map(f32::to_bits),
            placement.rotation().map(f32::to_bits)
        );
        assert_eq!(
            placement.rotation().map(f32::to_bits).as_slice(),
            &bits[8..12],
            "quaternion case {count}"
        );
        assert_eq!(
            placement
                .matrix()
                .to_cols_array()
                .map(f32::to_bits)
                .as_slice(),
            &bits[12..],
            "matrix case {count}"
        );
        // 4F45B0 reads the current behavior-packed LOCAL rotation. The animated
        // matrix is already in world space and must not acquire its parent twice.
        let parent_matrix = resolver.resolve(&world, 2)?.matrix();
        world.update_game_object_animated_pose(
            2,
            GameObjectAnimatedPose::new(parent_matrix, parent_packed),
        )?;
        world.update_transform(2, WorldTransform::new(Vec3::splat(-999.), 0.))?;
        let mut animated_matrix = placement.matrix();
        animated_matrix.w_axis += Vec4::new(100., 200., 300., 0.);
        world.update_game_object_animated_pose(
            3,
            GameObjectAnimatedPose::new(animated_matrix, local_packed),
        )?;
        let animated = resolver.resolve(&world, 3)?;
        assert_eq!(
            animated.matrix(),
            animated_matrix,
            "animated passenger case {count}"
        );
        assert_eq!(animated.rotation(), placement.rotation());
        assert_eq!(animated.facing(), placement.facing());
        let base = resolver.resolve_animation_base(&world, 3)?;
        assert_eq!(base.rotation(), placement.rotation());
        assert_eq!(base.matrix().w_axis, placement.matrix().w_axis);
        assert_eq!(resolver.resolve(&world, 3)?, animated);
        // Retire this sample before supplying the next native fixture's inputs.
        world
            .storage_mut()
            .delete_component::<(GameObjectAnimatedPose,)>(child);
        world
            .storage_mut()
            .delete_component::<(GameObjectAnimatedPose,)>(parent);
        count += 1;
    }
    assert_eq!(count, 96);
    Ok(())
}
use std::error::Error;

#[test]
fn packed_rotations_and_matrices_match_original_machine_code() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/game-object-rotation-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let packed = u64::from_str_radix(words.next().ok_or("missing packed value")?, 16)?;
        let expected = words
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(expected.len(), 20);
        let rotation = unpack_game_object_rotation(packed);
        for (actual, expected) in rotation.into_iter().zip(&expected) {
            if f32::from_bits(*expected).is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), *expected, "packed {packed:016x}");
            }
        }
        if rotation.iter().all(|v| v.is_finite()) {
            let placement = GameObjectPlacement::new(Vec3::ZERO, rotation, 1.0)?;
            for (index, (actual, expected)) in placement
                .matrix()
                .to_cols_array()
                .into_iter()
                .zip(&expected[4..])
                .enumerate()
            {
                assert_eq!(
                    actual.to_bits(),
                    *expected,
                    "matrix[{index}], packed {packed:016x}"
                );
            }
        } else {
            assert_eq!(
                GameObjectPlacement::new(Vec3::ZERO, rotation, 1.0),
                Err(GameObjectPlacementError::InvalidTransform)
            );
        }
        count += 1;
    }
    assert_eq!(count, 540);
    Ok(())
}

#[test]
fn passenger_chain_uses_current_parent_and_rejects_missing_or_cyclic_owners()
-> Result<(), Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Placement",
        Vec3::ZERO,
        0.0,
    ));
    let mut resolver = GameObjectPlacementResolver::default();
    let parent = 2;
    let passenger = 3;
    for (guid, position, scale) in [
        (parent, Vec3::new(10.0, 20.0, 30.0), 2.0),
        (passenger, Vec3::splat(-999.0), 0.5),
    ] {
        let entity = world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(position, 2.5)),
            [],
        )?;
        world
            .storage_mut()
            .add_component(entity, (ObjectPresentation::new(1, scale),));
    }
    world.update_game_object_movement(
        passenger,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: parent,
                position: Vec3::new(2.0, 3.0, 4.0),
                orientation: -1.0,
            }),
        ),
    )?;
    let placement = resolver.resolve(&world, passenger)?;
    assert_eq!(
        placement.matrix().w_axis.truncate(),
        Vec3::new(14.0, 26.0, 38.0)
    );
    assert_eq!(placement.matrix().x_axis.truncate(), Vec3::X * 0.5);
    // Packed zero is identity even when the ordinary movement facing is nonzero.
    assert_eq!(placement.rotation(), [0.0, 0.0, 0.0, 1.0]);
    world.update_transform(
        parent,
        WorldTransform::new(Vec3::new(100.0, 200.0, 300.0), 0.0),
    )?;
    assert_eq!(
        resolver
            .resolve(&world, passenger)?
            .matrix()
            .w_axis
            .truncate(),
        Vec3::new(104.0, 206.0, 308.0)
    );
    world.update_game_object_movement(
        parent,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: passenger,
                position: Vec3::ZERO,
                orientation: 0.0,
            }),
        ),
    )?;
    assert_eq!(
        resolver.resolve(&world, passenger),
        Err(GameObjectPlacementError::PassengerCycle { guid: passenger })
    );
    world.remove_object(parent)?;
    assert_eq!(
        resolver.resolve(&world, passenger),
        Err(GameObjectPlacementError::MissingObject { guid: parent })
    );
    world.update_game_object_movement(passenger, GameObjectMovement::default())?;
    assert_eq!(
        resolver
            .resolve(&world, passenger)?
            .matrix()
            .w_axis
            .truncate(),
        Vec3::splat(-999.0)
    );
    Ok(())
}
