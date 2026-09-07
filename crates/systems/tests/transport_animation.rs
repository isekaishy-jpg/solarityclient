//! Original-executable type-11 clock traces; geometry providers are excluded.

use std::error::Error;
use std::num::NonZeroU32;

use glam::{Mat4, Vec3};
use solarity_asset::{TransportAnimationNode, TransportRotationNode};
use solarity_systems::{TransportAnimationClock, TransportAnimationError, TransportAnimationTrack};

#[test]
fn stalled_frame_start_uses_the_native_signed_anchor_comparison() -> Result<(), Box<dyn Error>> {
    let period = Some(NonZeroU32::new(1000).ok_or("period")?);
    let clock = TransportAnimationClock::new(period, 0, 400, 700, 0);
    assert_eq!(clock.frame_start_ms(400, 900), 700);
    assert_eq!(clock.frame_start_ms(0, 900), 650);
    assert_eq!(clock.frame_start_ms(400, 1000), 750);
    let wrapped = TransportAnimationClock::new(period, 0, 400, 20, 0);
    assert_eq!(wrapped.frame_start_ms(400, 100), 20);
    let signed = TransportAnimationClock::new(period, 0, 400, 0x8000_0001, 0);
    assert_eq!(signed.frame_start_ms(400, 251), 1);
    Ok(())
}

#[test]
fn transport_animation_clock_matches_native_creation_reversals_and_endpoints()
-> Result<(), Box<dyn Error>> {
    let mut clock = None;
    let mut split = 0;
    let mut state = 0;
    let mut count = 0;
    for (line_number, line) in include_str!("fixtures/transport-animation-clock-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let columns: Vec<_> = line.split_whitespace().collect();
        if columns[0] == "case" {
            split = columns[2].parse()?;
            state = columns[3].parse()?;
            clock = Some(TransportAnimationClock::new(
                NonZeroU32::new(columns[1].parse()?),
                state,
                split,
                columns[4].parse()?,
                columns[5].parse()?,
            ));
            count += 1;
            continue;
        }
        let clock = clock.as_mut().ok_or("clock fixture has no initial case")?;
        let raw = columns[1].parse()?;
        match columns[0] {
            "state" => {
                let new = columns[2].parse()?;
                clock.notify_state(split, state, new, raw);
                state = new;
            }
            "split" => split = columns[2].parse()?,
            "sample" => assert_eq!(
                clock.sample_phase_ms(split, state, raw),
                columns[3].parse::<u32>()?,
                "sample at fixture line {}",
                line_number + 1,
            ),
            kind => panic!("unknown fixture operation {kind}"),
        }
        assert_eq!(
            clock.phase_ms(split, state, raw),
            columns[4].parse::<u32>()?,
            "phase at fixture line {}",
            line_number + 1
        );
        assert_eq!(
            clock.passenger_phase_ms(split, raw),
            columns[5].parse::<u32>()?,
            "passenger at fixture line {}",
            line_number + 1
        );
    }
    assert_eq!(count, 216);
    Ok(())
}

#[test]
fn transport_animation_tracks_match_native_offsets_rotations_and_wrap_intervals()
-> Result<(), Box<dyn Error>> {
    let mut track = None;
    let mut positions = Vec::new();
    let mut rotations = Vec::new();
    let mut parent = [0.; 4];
    let mut current = [0.; 4];
    let mut samples = 0;
    for (line_number, line) in include_str!("fixtures/transport-animation-track-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let columns: Vec<_> = line.split_whitespace().collect();
        match columns[0] {
            "case" => {
                positions.clear();
                rotations.clear();
                track = None;
                parent = floats(&columns[3..])?;
                current = floats(&columns[7..])?;
            }
            "position" => positions.push(TransportAnimationNode {
                id: positions.len() as u32 + 1,
                entry: 42,
                time_ms: columns[1].parse()?,
                sequence_id: columns[2].parse()?,
                position: floats(&columns[3..])?,
            }),
            "rotation" => rotations.push(TransportRotationNode {
                id: rotations.len() as u32 + 1,
                entry: 42,
                time_ms: columns[1].parse()?,
                rotation: floats(&columns[2..])?,
            }),
            "sample" => {
                if track.is_none() {
                    track = Some(TransportAnimationTrack::new(&positions, &rotations)?);
                }
                let track = track.as_mut().ok_or("missing track")?;
                let sample = track.sample(columns[1].parse()?, parent, current)?;
                assert_eq!(
                    sample.sequence_id,
                    (columns[2] != "-")
                        .then(|| columns[2].parse())
                        .transpose()?
                );
                let actual: Vec<_> = sample
                    .offset
                    .to_array()
                    .into_iter()
                    .chain(sample.rotation)
                    .collect();
                for (axis, value) in actual.into_iter().enumerate() {
                    assert_eq!(
                        value.to_bits(),
                        float(columns[3 + axis])?.to_bits(),
                        "geometry line {}, component {axis}",
                        line_number + 1
                    );
                }
                let expected_matrix = Mat4::from_cols_array(&floats(&columns[11..])?);
                if expected_matrix.is_finite() && expected_matrix.determinant() != 0.0 {
                    let pose = sample.pose(Vec3::ZERO)?;
                    assert_eq!(
                        pose.packed_rotation(),
                        u64::from_str_radix(columns[10], 16)?
                    );
                    assert_eq!(
                        pose.matrix().to_cols_array().map(f32::to_bits),
                        expected_matrix.to_cols_array().map(f32::to_bits),
                        "packed pose at line {}",
                        line_number + 1
                    );
                } else {
                    assert!(sample.pose(Vec3::ZERO).is_err());
                }
                samples += 1;
            }
            kind => panic!("unknown fixture operation {kind}"),
        }
    }
    assert_eq!(samples, 2520);
    Ok(())
}

#[test]
fn absent_single_and_uncovered_keys_preserve_native_selection_rules() -> Result<(), Box<dyn Error>>
{
    let position = |time_ms| TransportAnimationNode {
        id: 1,
        entry: 42,
        time_ms,
        position: [10., 20., 30.],
        sequence_id: 17,
    };
    assert!(matches!(
        TransportAnimationTrack::new(&[position(0)], &[]),
        Err(TransportAnimationError::ZeroPeriod)
    ));
    let current = [0., 0., 0., 1.];
    let mut single = TransportAnimationTrack::new(&[position(1000)], &[])?;
    let sample = single.sample(17, current, current)?;
    assert_eq!(sample.offset, Vec3::ZERO);
    assert_eq!(sample.sequence_id, None);
    let mut uncovered = TransportAnimationTrack::new(&[position(100), position(1000)], &[])?;
    assert!(matches!(
        uncovered.sample(0, current, current),
        Err(TransportAnimationError::MissingInterval {
            track: "position",
            phase_ms: 0
        })
    ));
    assert!(matches!(
        uncovered.sample(1000, current, current),
        Err(TransportAnimationError::MissingInterval {
            track: "position",
            phase_ms: 1000
        })
    ));
    Ok(())
}

fn float(word: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}

fn floats<const N: usize>(words: &[&str]) -> Result<[f32; N], Box<dyn Error>> {
    let mut values = [0.; N];
    for (value, word) in values.iter_mut().zip(words) {
        *value = float(word)?;
    }
    Ok(values)
}
