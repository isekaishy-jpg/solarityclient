//! External stock-compatibility tests for `rendering/particle` belong here.

use solarity_rendering::M2ParticleRandom;

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
