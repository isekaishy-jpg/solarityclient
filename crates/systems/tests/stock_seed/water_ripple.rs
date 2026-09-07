//! Original unit ripple requests, independent of the Rust emitter.

use glam::Vec3;
use solarity_systems::{
    WaterRipple, WaterRippleClock, WaterRippleEmission, WaterRippleEnvelope, WaterRippleOwner,
    WaterRipplePool, WaterRippleUnit,
};
use std::error::Error;

/// Native requests cover dry rejection, depth limits, movement flags, and clock wrap.
#[test]
fn unit_ripple_emissions_match_original_executable() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/water_ripple_emission.bin");
    assert_eq!(fixture.len(), 1033 * 104);
    for (index, bytes) in fixture.as_chunks::<104>().0.iter().enumerate() {
        let words: [u32; 26] =
            std::array::from_fn(|i| u32::from_le_bytes(bytes.as_chunks::<4>().0[i]));
        let mut clock = WaterRippleClock {
            next_emission_ms: words[4],
        };
        let mut draws = 0;
        let emission = clock.emit(
            WaterRippleUnit {
                position: Vec3::new(10., 20., f32::from_bits(words[10])),
                surface: (words[2] & 0x20 != 0).then_some(f32::from_bits(words[11])),
                permits_ripples: words[2] & 0x100 != 0,
                height: f32::from_bits(words[6]),
                scale: f32::from_bits(words[7]),
                speed: f32::from_bits(words[8]),
                yaw: f32::from_bits(words[9]),
                movement_flags: words[0],
                local_player: words[3] != 0,
            },
            words[1],
            words[5],
            || {
                draws += 1;
                words[12]
            },
        )?;
        let mut actual = [0_u32; 13];
        actual[..3].copy_from_slice(&[
            draws,
            clock.next_emission_ms,
            u32::from(emission.is_some()),
        ]);
        if let Some(emission) = emission {
            actual[3..11].copy_from_slice(
                &[
                    emission.position.x,
                    emission.position.y,
                    emission.position.z,
                    emission.yaw,
                    emission.radius,
                    emission.lifetime,
                    emission.strength,
                    emission.growth,
                ]
                .map(f32::to_bits),
            );
            actual[11] = u32::from(emission.directional);
            actual[12] = u32::from(emission.local_player);
        }
        assert_eq!(
            actual,
            words[13..],
            "native ripple case {index}: {:x?}",
            &words[..13]
        );
    }
    Ok(())
}

/// Native frame sequences cover envelope initialization, crossing, and retirement.
#[test]
fn ripple_envelopes_match_original_executable() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/water_ripple_lifecycle.bin");
    let mut words = fixture
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word));
    let cases = words.next().ok_or("missing native case count")?;
    assert_eq!(cases, 252);
    for case in 0..cases {
        let inputs: [u32; 10] = read_words(&mut words)?;
        let scalars = inputs.map(f32::from_bits);
        let mut envelope = WaterRippleEnvelope::new(
            WaterRippleEmission {
                position: Vec3::new(scalars[0], scalars[1], scalars[2]),
                yaw: scalars[3],
                radius: scalars[4],
                strength: scalars[5],
                lifetime: scalars[6],
                growth: scalars[7],
                directional: inputs[9] != 0,
                local_player: true,
            },
            scalars[8],
        )?;
        let initial = read_words(&mut words)?;
        assert_envelope_record(&envelope, initial, case, 0);
        let bounds: [u32; 6] = read_words(&mut words)?;
        assert_eq!(
            envelope
                .surface_bounds()
                .map(|bound| bound.to_array())
                .concat()
                .into_iter()
                .map(f32::to_bits)
                .collect::<Vec<_>>(),
            bounds,
            "native bounds case {case}",
        );
        let frames = words.next().ok_or("missing native frame count")?;
        for frame in 0..frames {
            let elapsed = f32::from_bits(words.next().ok_or("missing frame delta")?);
            let now = f32::from_bits(words.next().ok_or("missing frame time")?);
            let expected = read_words(&mut words)?;
            let alive = words.next().ok_or("missing native retirement result")? != 0;
            assert_eq!(
                envelope.advance(elapsed, now)?,
                alive,
                "case {case} frame {frame}"
            );
            assert_envelope_record(&envelope, expected, case, frame + 1);
            if !alive {
                assert_eq!(frame + 1, frames);
                assert!(!envelope.advance(0., now)?);
            }
        }
    }
    assert!(words.next().is_none(), "unconsumed native envelope data");
    Ok(())
}

/// Reads one complete fixed-width original record without accepting truncation.
fn read_words<const N: usize>(
    words: &mut impl Iterator<Item = u32>,
) -> Result<[u32; N], Box<dyn Error>> {
    let mut result = [0; N];
    for word in &mut result {
        *word = words.next().ok_or("truncated native ripple record")?;
    }
    Ok(result)
}

/// Compare observable projection and opacity fields without exposing private rates.
fn assert_envelope_record(envelope: &WaterRippleEnvelope, words: [u32; 12], case: u32, frame: u32) {
    let actual = [
        envelope.position().x.to_bits(),
        envelope.position().y.to_bits(),
        envelope.position().z.to_bits(),
        envelope.yaw().to_bits(),
        envelope.radius().to_bits(),
        envelope.opacity().to_bits(),
        u32::from(envelope.directional()),
    ];
    assert_eq!(
        actual,
        [
            words[0], words[1], words[2], words[3], words[4], words[6], words[11]
        ],
        "native envelope case {case} frame {frame}"
    );
}

/// Original list insertion establishes both bank capacity and reuse order.
#[test]
fn ripple_pool_reuse_and_active_order_match_original_executable() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/water_ripple_pool.bin");
    let mut words = fixture
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word));
    let cases = words.next().ok_or("missing native pool case count")?;
    assert_eq!(cases, 270);
    let mut pool = WaterRipplePool::default();
    for case in 0..cases {
        let local = words.next().ok_or("missing native owner")? != 0;
        let x = f32::from_bits(words.next().ok_or("missing native position")?);
        let count = words.next().ok_or("missing native active count")?;
        let expected: Vec<_> = words.by_ref().take(count as usize).collect();
        assert_eq!(expected.len(), count as usize);
        pool.insert(
            WaterRipple::new(
                WaterRippleEnvelope::new(
                    WaterRippleEmission {
                        position: Vec3::new(x, 20., 0.25),
                        yaw: 0.25,
                        radius: 0.33333334,
                        strength: 0.16666667,
                        lifetime: 0.65,
                        growth: 1.,
                        directional: false,
                        local_player: local,
                    },
                    0.,
                )?,
                Box::new([]),
            ),
            if local {
                WaterRippleOwner::LocalPlayer
            } else {
                WaterRippleOwner::OtherUnit
            },
        );
        assert_eq!(
            pool.ripples()
                .map(|ripple| ripple.envelope().position().x.to_bits())
                .collect::<Vec<_>>(),
            expected,
            "native pool insertion {case}"
        );
    }
    assert!(words.next().is_none());
    assert_eq!(pool.ripples().count(), 128);
    assert!(pool.advance(f32::NAN, 0.).is_err());
    assert_eq!(pool.ripples().count(), 128);
    pool.advance(1., 1.)?;
    assert_eq!(pool.ripples().count(), 0);
    Ok(())
}
