//! Native movement clock conversion boundaries.

const SECONDS_PER_MILLISECOND: f32 = f32::from_bits(0x3a83_126f);

/// FILD retains the complete unsigned clock through multiplication.
pub(super) fn seconds_from_millis(milliseconds: u32) -> f32 {
    (f64::from(milliseconds) * f64::from(SECONDS_PER_MILLISECOND)) as f32
}

/// FSTP float then FISTP signed integer, with round-to-nearest/even and the
/// x87 indefinite integer for overflow. The caller uses its raw unsigned bits.
pub(super) fn millis_from_seconds(seconds: f64) -> u32 {
    let milliseconds = f64::from((seconds * 1000.0) as f32).round_ties_even();
    if (-2147483648.0..2147483648.0).contains(&milliseconds) {
        milliseconds as i32 as u32
    } else {
        i32::MIN as u32
    }
}
