//! Original transport Euler matrix and canonical packed-rotation regression.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, GameObjectAnimatedPose, GameObjectMovement, GameObjectTransport, ObjectKind,
    ObjectPresentation, WorldBootstrap, WorldMapId, WorldTransform,
};
use solarity_systems::{
    GameObjectPlacementError, GameObjectPlacementResolver, game_object_transport_pose,
    unpack_game_object_rotation,
};

#[test]
fn initial_map_handle_uses_native_facing_and_ignores_own_scale() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/transport-initial-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words: Vec<_> = line.split_whitespace().collect();
        let packed = u64::from_str_radix(words[0], 16)?;
        let parent = (words[1] != "-")
            .then(|| u64::from_str_radix(words[1], 16))
            .transpose()?;
        let facing = f32::from_bits(u32::from_str_radix(words[2], 16)?);
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            1,
            "Initial",
            Vec3::ZERO,
            0.,
        ));
        for (guid, rotation, scale) in [(2, parent.unwrap_or_default(), 2.), (3, packed, 3.)] {
            let entity = world.create_object(
                guid,
                ObjectKind::GameObject,
                Some(WorldTransform::new(Vec3::new(10., 20., 30.), 2.5)),
                [],
            )?;
            world
                .storage_mut()
                .add_component(entity, (ObjectPresentation::new(1, scale),));
            world.update_game_object_movement(
                guid,
                GameObjectMovement::new(
                    rotation,
                    (guid == 3 && parent.is_some()).then_some(GameObjectTransport {
                        guid: 2,
                        position: Vec3::new(1., 2., 3.),
                        orientation: 2.5,
                    }),
                ),
            )?;
        }
        let mut resolver = GameObjectPlacementResolver::default();
        let replicated = resolver.resolve(&world, 3)?;
        assert_eq!(
            replicated.facing().to_bits(),
            facing.to_bits(),
            "virtual facing {count}"
        );
        let initial = resolver.resolve_map_model_initial(&world, 3)?;
        let expected =
            game_object_transport_pose(replicated.matrix().w_axis.truncate(), facing, 0., 0.)?;
        assert!(
            initial.matrix().abs_diff_eq(expected.matrix(), 0.000001),
            "case {count}: {line}"
        );
        // Cache entries from initial admission must not erase the replicated scale.
        assert_eq!(resolver.resolve(&world, 3)?, replicated);
        count += 1;
    }
    assert_eq!(count, 656);
    Ok(())
}

#[test]
fn transport_poses_match_original_matrix_and_quaternion_code() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/transport-pose-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let inputs = words[..6]
            .iter()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let pose = game_object_transport_pose(
            Vec3::from_slice(&inputs[..3]),
            inputs[3],
            inputs[4],
            inputs[5],
        )?;
        assert_eq!(
            pose.packed_rotation(),
            u64::from_str_radix(words[6], 16)?,
            "packed case {count}"
        );
        for (actual, expected) in pose.matrix().to_cols_array().into_iter().zip(&words[7..]) {
            let expected = f32::from_bits(u32::from_str_radix(expected, 16)?);
            assert!(
                (actual - expected).abs() <= 1e-7,
                "case {count}: {actual} != {expected}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 528);
    Ok(())
}

#[test]
fn animated_parent_drives_passenger_and_invalidates_cache_without_changing_replication()
-> Result<(), Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Transport",
        Vec3::ZERO,
        0.0,
    ));
    for guid in [2, 3] {
        let entity = world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(Vec3::splat(-999.0), 0.0)),
            [],
        )?;
        world.storage_mut().add_component(
            entity,
            (ObjectPresentation::new(
                1,
                if guid == 2 { 2.0 } else { 1.0 },
            ),),
        );
    }
    world.update_game_object_movement(2, GameObjectMovement::new(0, None))?;
    world.update_game_object_movement(
        3,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 2,
                position: Vec3::new(10.0, 20.0, 30.0),
                orientation: 0.0,
            }),
        ),
    )?;
    let pose = game_object_transport_pose(Vec3::new(100.0, 200.0, 300.0), 1.0, 0.3, 0.2)?;
    world.update_game_object_animated_pose(2, pose)?;
    let mut resolver = GameObjectPlacementResolver::default();
    let parent = resolver.resolve(&world, 2)?;
    assert_eq!(parent.matrix(), pose.matrix());
    let passenger = resolver.resolve(&world, 3)?;
    assert_eq!(
        passenger.rotation(),
        unpack_game_object_rotation(pose.packed_rotation())
    );
    let expected_position = pose.matrix().transform_point3(Vec3::new(10.0, 20.0, 30.0));
    assert!(
        passenger
            .matrix()
            .w_axis
            .truncate()
            .abs_diff_eq(expected_position, 0.00004)
    );
    // Preserve packed rotation while changing the full matrix: quantization
    // must not prevent either the root or its passenger from moving.
    let mut next_matrix = pose.matrix();
    next_matrix.w_axis.x += 10.0;
    world.update_game_object_animated_pose(
        2,
        GameObjectAnimatedPose::new(next_matrix, pose.packed_rotation()),
    )?;
    let moved = resolver.resolve(&world, 3)?;
    assert!((moved.matrix().w_axis.x - passenger.matrix().w_axis.x - 10.0).abs() < 0.00002);
    assert_eq!(
        world.game_object_movement(2),
        Some(GameObjectMovement::new(0, None))
    );
    assert_eq!(
        world
            .object_transform(2)
            .ok_or("missing transform")?
            .position(),
        Vec3::splat(-999.0)
    );
    world.remove_object(2)?;
    assert_eq!(world.game_object_animated_pose(2), None);
    assert_eq!(
        resolver.resolve(&world, 3),
        Err(GameObjectPlacementError::MissingObject { guid: 2 })
    );
    Ok(())
}
