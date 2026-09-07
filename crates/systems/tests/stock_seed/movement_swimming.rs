//! Original 987EF0 and 987B50 outputs, independent of the Rust implementation.

#[path = "movement_swimming/liquid_geometry.rs"]
mod liquid_geometry;

use std::error::Error;

use glam::Vec3;
use solarity_systems::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementGeometry, MovementSwimGeometry,
    MovementSwimInterval,
};

use solarity_ecs::WorldMovementSpeeds;
use solarity_systems::MovementSwimTrajectory;

#[test]
fn swim_immersion_notifications_match_original_executable() -> Result<(), Box<dyn Error>> {
    use solarity_systems::{MovementSwimImmersion, MovementSwimTransition};
    let fixture = include_bytes!("../fixtures/movement_swim_state.bin");
    assert_eq!(fixture.len(), 1328 * 68);
    for (index, bytes) in fixture.as_chunks::<68>().0.iter().enumerate() {
        let words: [u32; 17] =
            std::array::from_fn(|i| u32::from_le_bytes(bytes.as_chunks::<4>().0[i]));
        let update = MovementSwimImmersion {
            flags: words[0],
            secondary: words[1] as u16,
            unit_flags: words[2],
            parent_guid: u64::from(words[3]),
            locally_controlled: words[4] != 0,
            height: f32::from_bits(words[5]),
            liquid_depth: (words[6] != 0).then_some(f32::from_bits(words[7])),
            previous_depth: f32::from_bits(words[8]),
            fall_time_ms: words[9],
            initial_downward_speed: f32::from_bits(words[10]),
        }
        .evaluate()?;
        let actual = update.map_or([0, 0, 0, 0, 0, words[8]], |update| {
            [
                u32::from(update.transition == Some(MovementSwimTransition::Enter)),
                u32::from(update.transition == Some(MovementSwimTransition::Leave)),
                u32::from(update.attempt_surface_jump),
                u32::from(update.splash),
                u32::from(update.is_swimming) * 0x200000,
                update.previous_depth.to_bits(),
            ]
        });
        assert_eq!(
            actual,
            words[11..17],
            "case {index} inputs={:x?}",
            &words[..11]
        );
    }
    Ok(())
}

#[test]
fn swim_basis_and_analytic_motion_match_original_executable() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/movement_swim_trajectory.bin");
    assert_eq!(fixture.len(), 7290 * 64);
    let turn = f32::from_bits(0x4049_0fdb);
    let speeds = WorldMovementSpeeds::new([2.5, 7., 4.5, 4.72, 2.5, 7., 4.5, turn, turn]);
    for (index, bytes) in fixture.as_chunks::<64>().0.iter().enumerate() {
        let words: [u32; 16] =
            std::array::from_fn(|i| u32::from_le_bytes(bytes.as_chunks::<4>().0[i]));
        let trajectory = MovementSwimTrajectory::new(
            words[0],
            words[1] as u16,
            f32::from_bits(words[2]),
            f32::from_bits(words[3]),
            speeds,
        )?;
        let sample = trajectory.sample(words[4]);
        let actual = [
            trajectory.direction().x,
            trajectory.direction().y,
            trajectory.direction().z,
            trajectory.speed(),
            sample.displacement.x,
            sample.displacement.y,
            sample.displacement.z,
            sample.orientation,
            sample.pitch,
        ];
        let expected = [
            words[5], words[6], words[7], words[10], words[11], words[12], words[13], words[14],
            words[15],
        ];
        for (field, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert_eq!(
                actual.to_bits(),
                expected,
                "case {index} field {field}, flags={:#x}, time={} actual={actual}, expected={}",
                words[0],
                words[4],
                f32::from_bits(expected)
            );
        }
    }
    Ok(())
}

struct Geometry {
    ordinary: Vec<MovementCollisionTriangle>,
    water: Vec<MovementCollisionTriangle>,
    failure: u32,
    probes: u32,
}

impl MovementGeometry for Geometry {
    type TriangleIdentity = usize;
    fn prepare_sweep(&mut self, _: &MovementCollisionVolume, _: Vec3, _: f32) -> bool {
        self.probes += 1;
        self.probes != self.failure
    }
    fn triangles(&self) -> &[MovementCollisionTriangle] {
        &self.ordinary
    }
    fn triangle_identity(&self, index: usize) -> Option<usize> {
        Some(index)
    }
}
impl MovementSwimGeometry for Geometry {
    fn water_triangles(&self) -> &[MovementCollisionTriangle] {
        &self.water
    }
}

#[test]
fn swim_collision_and_surface_jump_match_original_executable() -> Result<(), Box<dyn Error>> {
    let mut fixture = include_bytes!("../fixtures/movement_swim_collision.bin").as_slice();
    let mut cases = 0;
    while !fixture.is_empty() {
        let input = take_words::<9>(&mut fixture);
        let mut geometry = Geometry {
            ordinary: take_triangles(&mut fixture, input[6])?,
            water: take_triangles(&mut fixture, input[7])?,
            failure: input[8],
            probes: 0,
        };
        let expected = take_words::<7>(&mut fixture);
        let actual = MovementSwimInterval {
            position: Vec3::ZERO,
            radius: 0.5,
            height: 2.,
            ascending: input[0] != 0,
            duration_ms: input[1],
            distance: f32::from_bits(input[2]),
            direction: Vec3::from_array([input[3], input[4], input[5]].map(f32::from_bits)),
        }
        .advance(&mut geometry)?;
        let result = [
            actual.consumed_ms,
            actual.position.x.to_bits(),
            actual.position.y.to_bits(),
            actual.position.z.to_bits(),
            u32::from(actual.reset_motion_anchor),
            u32::from(actual.attempt_surface_jump),
            u32::from(actual.geometry_unavailable),
        ];
        assert_eq!(
            result, expected,
            "case {cases}, input={input:?}, result={actual:?}"
        );
        cases += 1;
    }
    assert_eq!(cases, 348);
    Ok(())
}

fn take_words<const N: usize>(bytes: &mut &[u8]) -> [u32; N] {
    let (head, tail) = bytes.split_at(N * 4);
    *bytes = tail;
    std::array::from_fn(|i| u32::from_le_bytes(head.as_chunks::<4>().0[i]))
}

fn take_triangles(
    bytes: &mut &[u8],
    count: u32,
) -> Result<Vec<MovementCollisionTriangle>, solarity_systems::MovementSweepError> {
    (0..count)
        .map(|_| {
            let row = take_words::<13>(bytes).map(f32::from_bits);
            MovementCollisionTriangle::with_normal(
                [
                    Vec3::new(row[4], row[5], row[6]),
                    Vec3::new(row[7], row[8], row[9]),
                    Vec3::new(row[10], row[11], row[12]),
                ],
                Vec3::new(row[0], row[1], row[2]),
            )
        })
        .collect()
}
