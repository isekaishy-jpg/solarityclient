//! Blizzard's process-wide 32-bit table generator from build 12340.

/// Constant table read by the generator at executable address `0x009f1700`.
const RANDOM_TABLE: [u32; 61] = [
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
];

/// Global Blizzard random state seeded from the client timer at startup.
///
/// This is distinct from Visual C++ `rand`: sound selection and numerous game
/// systems call the generator at executable `0x00464580` through one shared
/// state object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlizzardRand {
    accumulator: u32,
    indices: [usize; 4],
}

impl BlizzardRand {
    /// Reproduces the seed transform at executable `0x004c1510`.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self {
            accumulator: seed,
            indices: [
                (seed % 61) as usize,
                (seed % 59) as usize,
                (seed % 53) as usize,
                (seed % 47) as usize,
            ],
        }
    }

    /// Advances the four prime-length cursors and returns one exact word.
    #[must_use]
    pub fn next_u32(&mut self) -> u32 {
        self.indices[0] = rewind(self.indices[0], 7, 61);
        self.indices[1] = rewind(self.indices[1], 6, 59);
        self.indices[2] = rewind(self.indices[2], 3, 53);
        self.indices[3] = rewind(self.indices[3], 1, 47);
        let mixed = RANDOM_TABLE[self.indices[3]].rotate_left(1)
            ^ RANDOM_TABLE[self.indices[2]].rotate_left(2)
            ^ RANDOM_TABLE[self.indices[1]].rotate_left(3)
            ^ RANDOM_TABLE[self.indices[0]];
        self.accumulator = self.accumulator.wrapping_add(mixed);
        self.accumulator
    }
}

/// Subtracts one cursor stride with the executable's prime-specific wrap.
const fn rewind(index: usize, stride: usize, modulus: usize) -> usize {
    if index < stride {
        index + modulus - stride
    } else {
        index - stride
    }
}
