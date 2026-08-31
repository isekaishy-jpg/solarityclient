//! Emitter-owned build-12340 random stream.

/// Executable bytes at `0x009F1700`, represented as aligned little-endian
/// words while preserving the stock generator's unaligned 32-bit reads.
const RANDOM_TABLE: [u32; 64] = [
    0x9927_148e,
    0x08c7_aafd,
    0x1f3e_e6d5,
    0xda55_bbf6,
    0x6a4a_a075,
    0xff97_bde8,
    0x9fbc_9bde,
    0x46a1_8a81,
    0x63e3_0b6e,
    0x5d6c_7a76,
    0xca69_d388,
    0x25b9_47c3,
    0x3fa2_ab83,
    0xba7c_41a6,
    0x0195_ace5,
    0xc109_cf7e,
    0x7170_62d9,
    0x0205_db8d,
    0x54ef_8724,
    0x3037_d4c6,
    0x7bcb_1bd0,
    0xecd8_e4b8,
    0xdcad_ce49,
    0xc494_a913,
    0x0dae_398f,
    0x0edd_5218,
    0x85f5_fa78,
    0x6daf_d258,
    0x3b53_b2a4,
    0xbe50_a551,
    0x11f4_2dfc,
    0xf116_9848,
    0x663d_df86,
    0x2f2e_445e,
    0x176b_0736,
    0xb64c_298b,
    0xe75f_89e2,
    0xe121_a7cd,
    0xed65_c94d,
    0x239c_eefe,
    0x04b7_7d33,
    0x402a_9a9e,
    0xf35b_10b3,
    0x921c_7782,
    0x571e_4e20,
    0x8c06_7222,
    0xfb73_2c67,
    0xbf0a_c259,
    0x0cf9_5c79,
    0x6812_1a28,
    0x4219_3474,
    0xf884_c0b1,
    0x9d15_f038,
    0x6f3a_f260,
    0x91eb_90b4,
    0x6135_7f1d,
    0x5603_325a,
    0x932b_c5a3,
    0x434b_0f80,
    0x3ce0_a8f7,
    0x2664_d196,
    0x4fcc_45d7,
    0xb5e9_b0c8,
    0xea31_d600,
];

/// One placement-local `CParticleEmitter` random stream.
///
/// The stock constructor combines two Visual C++ `rand()` results as
/// `(first << 16) | second`, then seeds this independent generator. Keeping the
/// combination outside this type preserves ownership: runtime owns the shared
/// CRT call order and each particle emitter owns all subsequent draws.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2ParticleRandom {
    value: u32,
    indices: u32,
}

impl M2ParticleRandom {
    /// Seeds the exact two-word state initialized at executable `0x004C1510`.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        let indices = (seed % 59) << 10
            | (seed % 61) << 2
            | (seed % 53) << 18
            | (seed.wrapping_add((seed / 47).wrapping_mul(17))) << 26;
        Self {
            value: seed,
            indices,
        }
    }

    /// Advances executable `0x00464580` and returns its raw 32-bit result.
    #[must_use]
    pub fn next_u32(&mut self) -> u32 {
        let first = decrement_cycle((self.indices & 0xff) as u8, 28, 216);
        let second = decrement_cycle(((self.indices >> 8) & 0xff) as u8, 24, 212);
        let third = decrement_cycle(((self.indices >> 16) & 0xff) as u8, 12, 200);
        let fourth = decrement_cycle((self.indices >> 24) as u8, 4, 184);
        let mixed = table_word(second).rotate_left(3)
            ^ table_word(third).rotate_left(2)
            ^ table_word(first)
            ^ table_word(fourth).rotate_left(1);
        self.indices = u32::from(first)
            | (u32::from(second) << 8)
            | (u32::from(third) << 16)
            | (u32::from(fourth) << 24);
        self.value = self.value.wrapping_add(mixed);
        self.value
    }

    /// Returns stock's bit-constructed uniform value in `[0, 1)`.
    #[must_use]
    pub fn next_unit(&mut self) -> f32 {
        let bits = self.next_u32() & 0x007f_ffff | 0x3f80_0000;
        f32::from_bits(bits) - 1.0
    }

    /// Returns stock's sign-bit-selected value in `(-1, 1)`.
    #[must_use]
    pub fn next_signed(&mut self) -> f32 {
        let random = self.next_u32();
        let magnitude = f32::from_bits(random & 0x007f_ffff | 0x3f80_0000);
        if random & 0x8000_0000 != 0 {
            2.0 - magnitude
        } else {
            magnitude - 2.0
        }
    }
}

/// Applies one branch of the four differently sized stock index cycles.
const fn decrement_cycle(value: u8, decrement: u8, wrapped_addition: u8) -> u8 {
    if value < decrement {
        value + wrapped_addition
    } else {
        value - decrement
    }
}

/// Reproduces an unaligned little-endian load from the executable byte table.
fn table_word(offset: u8) -> u32 {
    let offset = usize::from(offset);
    let word_index = offset / 4;
    let byte_shift = (offset % 4) * 8;
    let low = RANDOM_TABLE[word_index] >> byte_shift;
    if byte_shift == 0 {
        return low;
    }
    low | RANDOM_TABLE[word_index + 1] << (32 - byte_shift)
}
