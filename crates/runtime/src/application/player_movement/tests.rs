use super::*;
use solarity_systems::{MovementCollisionTriangle, MovementCollisionVolume, MovementGeometry};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn standing_and_control_changes_reconcile_held_input_and_packet_order() -> TestResult {
    let (mut world, mut owner) = owner()?;
    let entity = world.local_player();
    world.storage_mut().add_component(
        entity,
        (solarity_ecs::UnitVitals::new(100, 100, [0; 7], [0; 7]),),
    );
    let mut input = PlayerInputState::default();
    let mut output = VecDeque::new();
    owner.stand_state = 7;
    owner.command(
        MovementCommand::Input(UiMovementCommand {
            action: solarity_ui::UiMovementAction::Hold {
                control: solarity_ui::UiMovementControl::Forward,
                pressed: true,
            },
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    assert!(output.is_empty());
    owner.command(
        MovementCommand::Control(PlayerControlEvent::StandState {
            state: 0,
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    assert_eq!(owner.flags & 1, 1);
    assert!(
        matches!(output.pop_front(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::StartForward)
    );
    owner.command(
        MovementCommand::Control(PlayerControlEvent::PlayerControl {
            enabled: false,
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    owner.command(
        MovementCommand::Control(PlayerControlEvent::ActiveMover {
            previous: 1,
            next: 0,
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    owner.command(
        MovementCommand::Input(UiMovementCommand {
            action: solarity_ui::UiMovementAction::Hold {
                control: solarity_ui::UiMovementControl::Forward,
                pressed: false,
            },
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    assert_eq!(owner.flags & 0xff, 0);
    assert!(
        matches!(output.pop_front(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::NotActiveMover)
    );
    assert!(output.is_empty());
    owner.command(
        MovementCommand::Control(PlayerControlEvent::PlayerControl {
            enabled: true,
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    owner.command(
        MovementCommand::Control(PlayerControlEvent::ActiveMover {
            previous: 0,
            next: 1,
            timestamp_ms: 0,
        }),
        &mut input,
        &world,
        &mut output,
    )?;
    assert!(matches!(
        output.pop_front(),
        Some(PlayerMovementOutput::ActiveMover(1))
    ));
    assert!(
        matches!(output.pop_front(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::Heartbeat)
    );
    assert!(
        matches!(output.pop_front(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::Stop)
    );
    assert!(output.is_empty());
    Ok(())
}

#[test]
fn optimistic_stance_precedes_movement_and_control_loss_rejects_sitting() -> TestResult {
    let (mut world, mut owner) = owner()?;
    let entity = world.local_player();
    world.storage_mut().add_component(
        entity,
        (solarity_ecs::UnitVitals::new(100, 100, [0; 7], [0; 7]),),
    );
    let mut output = VecDeque::new();
    apply(&mut owner, &world, PlayerInputEffect::SitStand, &mut output)?;
    assert_eq!(owner.stand_state, 1);
    assert!(matches!(
        output.pop_front(),
        Some(PlayerMovementOutput::StandState(1))
    ));
    apply(
        &mut owner,
        &world,
        PlayerInputEffect::Movement(WorldMovementKind::StartForward),
        &mut output,
    )?;
    assert_eq!(owner.stand_state, 0);
    assert!(matches!(
        output.pop_front(),
        Some(PlayerMovementOutput::StandState(0))
    ));
    assert!(
        matches!(output.pop_front(), Some(PlayerMovementOutput::Movement(message)) if message.kind() == WorldMovementKind::StartForward)
    );
    owner.client_control = false;
    apply(&mut owner, &world, PlayerInputEffect::SitStand, &mut output)?;
    assert!(output.is_empty());
    assert_eq!(owner.stand_state, 0);
    assert_eq!(
        world.local_player_stand_state()?,
        0,
        "owner publishes only at the interval boundary"
    );
    Ok(())
}

#[test]
fn control_release_flags_match_original_response() -> TestResult {
    for line in include_str!("../../../tests/fixtures/player-control-release-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_whitespace().collect();
        let (_, mut owner) = owner()?;
        owner.flags = u32::from_str_radix(row[0], 16)?;
        owner.release_mover()?;
        assert_eq!(owner.flags, u32::from_str_radix(row[1], 16)?, "{line}");
        assert!(!owner.active);
    }
    Ok(())
}

#[test]
fn control_acquire_fall_matches_original_response() -> TestResult {
    for line in include_str!("../../../tests/fixtures/player-control-acquire-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_whitespace().collect();
        let (_, mut owner) = owner()?;
        owner.flags = u32::from_str_radix(row[0], 16)?;
        owner.secondary = u16::from_str_radix(row[1], 16)?;
        owner.position = Vec3::new(3., 4., 5.);
        if owner.flags & 0x0220_0000 != 0 || owner.secondary & 0x20 != 0 {
            owner.context.pitch_radians = Some(0.);
        }
        // Event 9 rejects a pending root before calling the captured 98B710.
        let admitted = owner.flags & 0x100000 == 0;
        let changed = row[2] == "1" && admitted;
        assert_eq!(owner.acquired_fall_flags().is_some(), changed, "{line}");
        if admitted {
            assert_eq!(
                owner.acquired_fall_flags().unwrap_or(owner.flags),
                u32::from_str_radix(row[3], 16)?,
                "{line}"
            );
        }
        // Pitch motion requires its own trajectory; capture its flag response
        // above without representing it as an ordinary ground/fall curve.
        if owner.flags & 0xc0 != 0 {
            continue;
        }
        let mut output = VecDeque::new();
        owner.acquire_mover(&mut output)?;
        assert_eq!(!output.is_empty(), changed, "{line}");
        if changed {
            let MovementPhase::Fall(fall) = owner.phase else {
                panic!("{line}");
            };
            assert_eq!(
                fall.snapshot().fall_time_ms,
                u32::from_str_radix(row[4], 16)?
            );
            assert_eq!(
                fall.snapshot().launch_height.to_bits(),
                u32::from_str_radix(row[5], 16)?
            );
            assert_eq!(
                fall.snapshot().initial_downward_speed.to_bits(),
                u32::from_str_radix(row[6], 16)?
            );
        }
    }
    Ok(())
}

#[test]
fn control_recovery_in_air_restarts_fall_and_lands() -> TestResult {
    let (world, mut owner) = owner()?;
    let mut output = VecDeque::new();
    let mut floor = Floor::new()?;
    apply(&mut owner, &world, PlayerInputEffect::Jump, &mut output)?;
    owner.advance_to(300, [0.5, 2., 1.], &mut floor, &mut output)?;
    let suspended = owner.position;
    assert!(suspended.z > 1.);
    owner.release_mover()?;
    owner.emit(WorldMovementKind::NotActiveMover, &mut output)?;
    owner.advance_to(700, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position, suspended);
    owner.active = true;
    owner.acquire_mover(&mut output)?;
    for time in (720..=1700).step_by(20) {
        owner.advance_to(time, [0.5, 2., 1.], &mut floor, &mut output)?;
    }
    assert!(owner.position.z.abs() < 0.002, "{:?}", owner.position);
    assert!(matches!(owner.phase, MovementPhase::Ground { .. }));
    let kinds: Vec<_> = output
        .iter()
        .filter_map(|event| match event {
            PlayerMovementOutput::Movement(message) => Some(message.kind()),
            _ => None,
        })
        .collect();
    let retired = kinds
        .iter()
        .position(|kind| *kind == WorldMovementKind::NotActiveMover)
        .ok_or("retired")?;
    assert_eq!(kinds[retired + 1], WorldMovementKind::Heartbeat);
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == WorldMovementKind::FallLand)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn deferred_landing_flags_match_original_resolver() -> TestResult {
    let (_, mut owner) = owner()?;
    for line in include_str!("../../../tests/fixtures/player-deferred-native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        owner.flags = u32::from_str_radix(fields[0], 16)?;
        owner.secondary = u16::from_str_radix(fields[1], 16)?;
        owner.apply_deferred();
        assert_eq!(owner.flags, u32::from_str_radix(fields[2], 16)?, "{line}");
    }
    Ok(())
}

struct Floor {
    triangles: Vec<MovementCollisionTriangle>,
    ready: bool,
}

impl Floor {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            triangles: vec![
                MovementCollisionTriangle::new([
                    Vec3::new(-100., -100., 0.),
                    Vec3::new(100., -100., 0.),
                    Vec3::new(100., 100., 0.),
                ])?,
                MovementCollisionTriangle::new([
                    Vec3::new(-100., -100., 0.),
                    Vec3::new(100., 100., 0.),
                    Vec3::new(-100., 100., 0.),
                ])?,
            ],
            ready: true,
        })
    }
}

impl MovementGeometry for Floor {
    type TriangleIdentity = usize;
    fn prepare_sweep(&mut self, _: &MovementCollisionVolume, _: Vec3, _: f32) -> bool {
        self.ready
    }
    fn triangles(&self) -> &[MovementCollisionTriangle] {
        &self.triangles
    }
    fn triangle_identity(&self, index: usize) -> Option<usize> {
        self.triangles.get(index).map(|_| index)
    }
}

impl LocalMovementGeometry for Floor {
    fn collect(
        &mut self,
        request: MovementIntervalRequest,
    ) -> Result<bool, RuntimePlayerMovementError> {
        request
            .collection_bounds()
            .map_err(super::super::RuntimeStaticMovementError::from)?;
        Ok(self.ready)
    }
}

fn owner() -> Result<(ActiveWorld, LocalMovement), Box<dyn std::error::Error>> {
    let world = ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(0),
        1,
        "Mover",
        Vec3::ZERO,
        0.,
    ));
    let movement = WorldMovementState::new(
        0,
        WorldMovementSpeeds::new([
            2.5,
            7.,
            4.5,
            4.72,
            2.5,
            7.,
            4.5,
            std::f32::consts::PI,
            std::f32::consts::PI,
        ]),
        WorldMovementContext::default(),
    );
    let owner = LocalMovement::new(
        world.object_identity(1).ok_or("identity")?,
        world.local_player_transform()?,
        movement,
        0,
    )?;
    Ok((world, owner))
}

fn apply(
    owner: &mut LocalMovement,
    world: &ActiveWorld,
    effect: PlayerInputEffect,
    output: &mut VecDeque<PlayerMovementOutput>,
) -> Result<(), RuntimePlayerMovementError> {
    owner.apply(
        effect,
        PlayerInputAdmission {
            translation: true,
            turning: true,
            forced_forward: false,
            yaw_during_mouselook: false,
            movement_flags: owner.flags,
            secondary_flags: owner.secondary,
        },
        world,
        output,
    )
}

#[test]
fn grounded_frames_sample_one_anchor_and_emit_heartbeat_deadlines() -> TestResult {
    let (world, mut one_ms) = owner()?;
    let (_, mut coarse) = owner()?;
    let mut fine_output = VecDeque::new();
    let mut coarse_output = VecDeque::new();
    for (owner, output) in [
        (&mut one_ms, &mut fine_output),
        (&mut coarse, &mut coarse_output),
    ] {
        apply(
            owner,
            &world,
            PlayerInputEffect::Movement(WorldMovementKind::StartForward),
            output,
        )?;
        apply(
            owner,
            &world,
            PlayerInputEffect::Movement(WorldMovementKind::StartTurnLeft),
            output,
        )?;
    }
    let mut floor = Floor::new()?;
    for time in 1..=1000 {
        one_ms.advance_to(time, [0.5, 2., 1.], &mut floor, &mut fine_output)?;
    }
    for time in (20..=1000).step_by(20) {
        coarse.advance_to(time, [0.5, 2., 1.], &mut floor, &mut coarse_output)?;
    }
    assert!(
        one_ms.position.distance(coarse.position) < 0.001,
        "{:?} {:?}",
        one_ms.position,
        coarse.position
    );
    assert_eq!(one_ms.orientation.to_bits(), coarse.orientation.to_bits());
    assert_eq!(one_ms.flags & 0x3000, 0);
    let heartbeats = fine_output.iter().filter(|event| matches!(event, PlayerMovementOutput::Movement(message) if message.kind() == WorldMovementKind::Heartbeat)).count();
    assert_eq!(heartbeats, 2);
    assert_eq!(one_ms.heartbeat_ms, 1500);
    Ok(())
}

#[test]
fn jump_retains_launch_until_landing_and_applies_released_forward() -> TestResult {
    let (world, mut owner) = owner()?;
    let mut output = VecDeque::new();
    let mut floor = Floor::new()?;
    apply(
        &mut owner,
        &world,
        PlayerInputEffect::Movement(WorldMovementKind::StartForward),
        &mut output,
    )?;
    apply(&mut owner, &world, PlayerInputEffect::Jump, &mut output)?;
    owner.advance_to(300, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!(owner.position.z > 1.);
    apply(
        &mut owner,
        &world,
        PlayerInputEffect::Movement(WorldMovementKind::Stop),
        &mut output,
    )?;
    assert_eq!(owner.flags & 0x5001, 0x5001);
    for time in (320..=1200).step_by(20) {
        owner.advance_to(time, [0.5, 2., 1.], &mut floor, &mut output)?;
    }
    assert!(matches!(owner.phase, MovementPhase::Ground { .. }));
    assert!(owner.position.z.abs() < 0.002, "{:?}", owner.position);
    assert!(
        owner.position.x > 4. && owner.position.x < 7.,
        "{:?}",
        owner.position
    );
    assert_eq!(owner.flags & 0x000f_f003, 0);
    assert_eq!(output.iter().filter(|event| matches!(event, PlayerMovementOutput::Movement(message) if message.kind() == WorldMovementKind::FallLand)).count(), 1);
    assert!(owner.snapshot().1.context().falling.is_none());
    Ok(())
}

#[test]
fn unavailable_interval_postpones_motion_and_heartbeat() -> TestResult {
    let (world, mut owner) = owner()?;
    let mut output = VecDeque::new();
    let mut floor = Floor::new()?;
    apply(
        &mut owner,
        &world,
        PlayerInputEffect::Movement(WorldMovementKind::StartForward),
        &mut output,
    )?;
    floor.ready = false;
    owner.advance_to(160, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert_eq!(owner.position, Vec3::ZERO);
    assert_eq!(owner.elapsed_ms, 0);
    assert_eq!(owner.heartbeat_ms, 660);
    assert!(matches!(
        output.back(),
        Some(PlayerMovementOutput::SkippedTime {
            guid: 1,
            milliseconds: 160
        })
    ));
    floor.ready = true;
    owner.advance_to(260, [0.5, 2., 1.], &mut floor, &mut output)?;
    assert!((owner.position.x - 0.7).abs() < 0.00001);
    Ok(())
}
