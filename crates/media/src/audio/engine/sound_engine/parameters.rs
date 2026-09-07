//! Authored volume and pitch randomization from 4C6FEE through 4C7062.

#[cfg(test)]
#[path = "../../../../tests/stock_seed/audio/engine/parameters.rs"]
mod tests;

pub(super) struct SelectedSoundParameters {
    pub(super) gain: f32,
    pub(super) frequency_ratio: f32,
}

pub(super) fn select_parameters(
    volume: f32,
    multiplier: f32,
    flags: u32,
    next_word: &mut impl FnMut() -> u32,
) -> SelectedSoundParameters {
    // Native stores the product to f32 before adding the x87 random offset.
    let base = volume * multiplier;
    let gain = if flags & 0x800 != 0 {
        (f64::from(base) + random_range(-0.15, 0.15, next_word())) as f32
    } else {
        base
    };
    // 879710 clamps the completed source gain, before category and fade gains.
    let gain = gain.clamp(0.0, 1.0);
    let frequency_ratio = if flags & 0x400 != 0 {
        random_range(0.85, 1.15, next_word()) as f32
    } else {
        1.0
    };
    SelectedSoundParameters {
        gain,
        frequency_ratio,
    }
}

fn random_range(minimum: f32, maximum: f32, word: u32) -> f64 {
    // 982310 fills the mantissa from the LOW 23 bits, not a normalized u32.
    let unit = f64::from(f32::from_bits((word & 0x7f_ffff) | 0x3f80_0000)) - 1.0;
    unit * f64::from(maximum - minimum) + f64::from(minimum)
}
