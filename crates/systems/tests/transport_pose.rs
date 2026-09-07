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
