//! External stock-compatibility tests for `rendering/particle` belong here.

use glam::Vec3;
use solarity_rendering::{M2ParticleRandom, M2ParticleState, M2ParticleStateError};

/// The emitter-owned table generator reproduces recovered executable vectors.
#[test]
fn particle_random_reproduces_stock_stream() {
    let mut random = M2ParticleRandom::new(0x0029_4823);
    assert_eq!(random.next_u32(), 0xc0d0_f1ae);
    assert_eq!(random.next_u32(), 0x5ca5_6410);
    assert_eq!(random.next_u32(), 0xefa3_1072);

    let mut unit = M2ParticleRandom::new(0x0029_4823);
    assert_eq!(unit.next_unit().to_bits(), 0x3f21_e35c);
    let mut signed = M2ParticleRandom::new(0x0029_4823);
    assert_eq!(signed.next_signed().to_bits(), 0x3ebc_3948);
}

/// Ordinary particle motion follows stock age, gravity, and drag ordering.
#[test]
fn particle_state_advances_stock_ballistics() -> Result<(), M2ParticleStateError> {
    let mut particle = M2ParticleState::new(0.0, Vec3::ZERO, Vec3::new(2.0, 4.0, 6.0), 0)?;
    particle.advance(0.5, 8.0, 0.25)?;

    assert_eq!(particle.age_seconds(), 0.5);
    assert_eq!(particle.position(), Vec3::new(1.0, 2.0, 2.0));
    assert_eq!(particle.velocity(), Vec3::new(1.75, 3.5, 1.75));
    assert_eq!(particle.lifetime_seconds(1.0, 0.5), 1.0);
    assert_eq!(particle.normalized_age(1.0, 0.5), 0.5);
    assert!(particle.is_alive(1.0, 0.5));

    particle.advance(0.5, 8.0, 0.25)?;
    assert!(!particle.is_alive(1.0, 0.5));
    assert_eq!(
        particle.advance(-0.1, 8.0, 0.25),
        Err(M2ParticleStateError::ElapsedTime)
    );
    Ok(())
}

/// The stored random word is reinterpreted as signed for lifetime variation.
#[test]
fn particle_state_uses_stock_signed_lifetime_word() -> Result<(), M2ParticleStateError> {
    let positive = M2ParticleState::new(0.0, Vec3::ZERO, Vec3::ZERO, 0x7fff)?;
    let negative = M2ParticleState::new(0.0, Vec3::ZERO, Vec3::ZERO, 0x8001)?;
    assert_eq!(positive.lifetime_seconds(2.0, 0.5), 2.5);
    assert_eq!(negative.lifetime_seconds(2.0, 0.5), 1.5);

    let clamped = M2ParticleState::new(0.0, Vec3::ZERO, Vec3::ZERO, 0x8000)?;
    assert_eq!(clamped.lifetime_seconds(0.0, 100.0).to_bits(), 0x3a83_126f);
    Ok(())
}
