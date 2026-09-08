//! Build 12340's four-key stars opacity curve (7EE0D0).

/// Returns the original packed alpha; values below two skip the stars model.
#[must_use]
pub fn world_stars_alpha(day_fraction: f32) -> u8 {
    const KEYS: [[f32; 2]; 4] = [[0.125, 1.0], [0.1875, 0.0], [0.9375, 0.0], [1.0, 1.0]];
    (super::sky::cyclic(&KEYS, day_fraction) * 254.0 + 1.0) as u8
}

#[cfg(test)]
mod tests {
    #[test]
    fn opacity_matches_unmodified_native_update() -> Result<(), Box<dyn std::error::Error>> {
        let mut samples = 0;
        for line in include_str!("../../tests/fixtures/world_stars_native.txt").lines() {
            if line.starts_with('#') {
                continue;
            }
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let day = f32::from_bits(u32::from_str_radix(fields[0], 16)?);
            let alpha = fields[1].parse::<u8>()?;
            assert_eq!(super::world_stars_alpha(day), alpha, "day={day:?}");
            samples += 1;
        }
        assert!(samples > 3000);
        Ok(())
    }
}
