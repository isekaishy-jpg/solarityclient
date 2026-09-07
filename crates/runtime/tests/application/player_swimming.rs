//! Runtime swimming transitions, packets and 3D collision ownership.

use super::super::tests::{Floor, owner};
use super::*;
use crate::application::player_movement::tests::apply;
use crate::input::{PlayerInputEffect, PlayerInputState};
use solarity_systems::MovementIntervalRequest;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn swimming_commands_retain_original_basis_speed_and_fall_launch() -> TestResult {
    let bytes = include_bytes!("../fixtures/movement_swim_transition.bin");
    let (records, remainder) = bytes.as_chunks::<52>();
    assert!(remainder.is_empty());
    assert_eq!(records.len(), 108);
    for (case, record) in records.iter().enumerate() {
        let (words, _) = record.as_chunks::<4>();
        let words: [u32; 13] = std::array::from_fn(|index| u32::from_le_bytes(words[index]));
        let (_, mut mover) = owner()?;
        mover.position = Vec3::new(10., 20., 30.);
        mover.orientation = 0.7;
        mover.flags = words[1];
        mover.secondary = words[2] as u16;
        mover.context.pitch_radians = Some(f32::from_bits(words[3]));
        if mover.flags & 0x200000 != 0 {
            mover.phase = MovementPhase::Swimming(mover.swim_trajectory()?);
        }
        mover.reanchor()?;
        let mut output = VecDeque::new();
        match words[0] {
            0 => mover.swim_transition(MovementSwimTransition::Enter, &mut output)?,
            1 => mover.swim_transition(MovementSwimTransition::Leave, &mut output)?,
            2 => mover.launch_swim_jump()?,
            _ => return Err("unexpected native command".into()),
        }
        let (direction, speed, clock, height, downward) = match mover.phase {
            MovementPhase::Swimming(trajectory) => {
                (trajectory.direction(), trajectory.speed(), 0, 0., 0.)
            }
            MovementPhase::Fall(fall) => {
                let state = fall.snapshot();
                (
                    state.direction,
                    state.horizontal_speed,
                    state.fall_time_ms,
                    state.launch_height,
                    state.initial_downward_speed,
                )
            }
            MovementPhase::Ground { .. } => return Err("swimming command selected ground".into()),
        };
        let actual = [
            mover.flags,
            mover.context.pitch_radians.unwrap_or(0.).to_bits(),
            direction.x.to_bits(),
            direction.y.to_bits(),
            direction.z.to_bits(),
            speed.to_bits(),
            clock,
            height.to_bits(),
            downward.to_bits(),
        ];
        assert_eq!(actual, words[4..], "original command {case}");
    }
    Ok(())
}

#[test]
fn swimming_steering_moves_in_three_dimensions_and_release_preserves_water_mode() -> TestResult {
    let (world, mut mover) = owner()?;
    mover.position.z = 20.;
    let mut output = VecDeque::new();
    mover.swim_transition(MovementSwimTransition::Enter, &mut output)?;
    apply(
        &mut mover,
        &world,
        PlayerInputEffect::Movement(WorldMovementKind::StartForward),
        &mut output,
    )?;
    let mut input = PlayerInputState::default();
    mover.set_mouse_facing(&mut input, &mut output)?;
    let pitch = mover
        .snapshot()
        .1
        .context()
        .pitch_radians
        .ok_or("swimming pitch")?;
    assert!(pitch < 0.);
    let expected = mover.swim_trajectory()?.sample(100).displacement;
    let origin = mover.position;
    let mut floor = Floor::new()?;
    mover.advance_to(100, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!((mover.position - origin - expected).length() < 0.00001);
    assert!(mover.position.z < origin.z);
    let kinds: Vec<_> = output
        .iter()
        .filter_map(|event| match event {
            PlayerMovementOutput::Movement(message) => Some(message.kind()),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            WorldMovementKind::StartSwim,
            WorldMovementKind::StartForward,
            WorldMovementKind::SetFacing,
            WorldMovementKind::SetPitch
        ]
    );
    mover.release_mover()?;
    assert!(matches!(mover.phase, MovementPhase::Swimming(_)));
    assert_eq!(mover.flags & 0xc010ff, 0);
    mover.active = true;
    mover.acquire_mover(&mut output)?;
    assert!(matches!(mover.phase, MovementPhase::Swimming(_)));
    Ok(())
}

#[test]
fn immersion_queues_entry_after_fall_and_exit_after_leaving_the_surface() -> TestResult {
    let (mut world, mut mover) = owner()?;
    let player = world.local_player();
    world
        .storage_mut()
        .add_component(player, (solarity_ecs::UnitFlags::new(8, 0, 0),));
    mover.position.z = 20.;
    let mut output = VecDeque::new();
    mover.acquire_mover(&mut output)?;
    assert!(matches!(mover.phase, MovementPhase::Fall(_)));
    let mut commands = VecDeque::from([MovementCommand::SupportRecheck { timestamp_ms: 1000 }]);
    mover.queue_immersion(
        Some(SubmergedLiquid {
            liquid_type: 1,
            surface_height: 23.,
            depth: 3.,
        }),
        2.,
        &world,
        &mut commands,
    )?;
    assert!(matches!(mover.phase, MovementPhase::Fall(_)));
    let mut input = PlayerInputState::default();
    mover.command(
        commands.pop_front().ok_or("queued water entry")?,
        &mut input,
        &world,
        &mut output,
    )?;
    assert!(matches!(mover.phase, MovementPhase::Swimming(_)));
    assert!(mover.snapshot().1.context().falling.is_none());
    assert_eq!(mover.flags & 0x203000, 0x200000);
    mover.queue_immersion(None, 2., &world, &mut commands)?;
    mover.command(
        commands.pop_front().ok_or("queued water exit")?,
        &mut input,
        &world,
        &mut output,
    )?;
    assert!(matches!(mover.phase, MovementPhase::Fall(_)));
    assert_eq!(mover.flags & 0x201000, 0x1000);
    assert!(mover.snapshot().1.context().pitch_radians.is_none());
    assert!(mover.snapshot().1.context().falling.is_some());
    assert!(matches!(
        commands.pop_front(),
        Some(MovementCommand::SupportRecheck { timestamp_ms: 1000 })
    ));
    assert!(commands.is_empty());
    Ok(())
}

/// Independent water bank with the stock downward plane and authored winding.
struct Pool {
    floor: Floor,
    water: Vec<MovementCollisionTriangle>,
}

impl MovementGeometry for Pool {
    type TriangleIdentity = usize;
    fn prepare_sweep(
        &mut self,
        volume: &MovementCollisionVolume,
        direction: Vec3,
        distance: f32,
    ) -> bool {
        self.floor.prepare_sweep(volume, direction, distance)
    }
    fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.floor.triangles()
    }
    fn triangle_identity(&self, index: usize) -> Option<usize> {
        self.floor.triangle_identity(index)
    }
}

impl LocalMovementGeometry for Pool {
    fn collect(
        &mut self,
        request: MovementIntervalRequest,
    ) -> Result<bool, RuntimePlayerMovementError> {
        self.floor.collect(request)
    }
    fn water_triangles(&self) -> &[MovementCollisionTriangle] {
        &self.water
    }
}

#[test]
fn ascending_surface_contact_uses_water_height_and_emits_a_jump() -> TestResult {
    let (world, mut mover) = owner()?;
    mover.position.z = 20.;
    let mut output = VecDeque::new();
    mover.swim_transition(MovementSwimTransition::Enter, &mut output)?;
    apply(
        &mut mover,
        &world,
        PlayerInputEffect::Movement(WorldMovementKind::StartAscend),
        &mut output,
    )?;
    let mut pool = Pool {
        floor: Floor::new()?,
        water: vec![
            MovementCollisionTriangle::with_normal(
                [
                    Vec3::new(-100., -100., 23.),
                    Vec3::new(100., -100., 23.),
                    Vec3::new(100., 100., 23.),
                ],
                -Vec3::Z,
            )?,
            MovementCollisionTriangle::with_normal(
                [
                    Vec3::new(-100., -100., 23.),
                    Vec3::new(100., 100., 23.),
                    Vec3::new(-100., 100., 23.),
                ],
                -Vec3::Z,
            )?,
        ],
    };
    mover.advance_to(400, [0.5, 2., 1.], &mut pool, &mut output)?;
    let MovementPhase::Fall(fall) = mover.phase else {
        return Err("surface jump did not start falling".into());
    };
    assert!((fall.snapshot().launch_height - 21.5).abs() < 0.01);
    assert_eq!(fall.snapshot().initial_downward_speed.to_bits(), 0xc1118c48);
    assert!(output.iter().any(|event| matches!(event, PlayerMovementOutput::Movement(message) if message.kind() == WorldMovementKind::Jump)));
    Ok(())
}
