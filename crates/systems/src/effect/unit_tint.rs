//! Unit model color from environmental SpellVisualKit special effect 13.

/// The native unit-owned hold/fade state, independent of model generations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnitModelTint {
    started: u32,
    color: u32,
    hold_ms: u32,
    fade_ms: u32,
}

impl UnitModelTint {
    /// Applies the authored color and second-based durations at 7265C0.
    pub fn apply(&mut self, now: u32, parameters: [f32; 4]) {
        self.started = now;
        self.color = truncate_word(parameters[0]) | 0xff00_0000;
        self.hold_ms = truncate_word(parameters[1] * 1000.0);
        self.fade_ms = truncate_word(parameters[2] * 1000.0);
    }

    /// Returns 720DB0's BGRA color, or the unit's ordinary model color on expiry.
    /// Active fades start from white and use 6ACC50's integer channel blend.
    pub fn sample(&mut self, now: u32, ordinary_color: u32) -> u32 {
        if self.started == 0 {
            return ordinary_color;
        }
        let hold_end = self.started.wrapping_add(self.hold_ms);
        let elapsed = now.wrapping_sub(hold_end);
        if (elapsed as i32) < 0 {
            return self.color;
        }
        if (elapsed.wrapping_sub(self.fade_ms) as i32) >= 0 {
            self.started = 0;
            return ordinary_color;
        }
        let weight = ((1.0 - f64::from(elapsed) / f64::from(self.fade_ms)) * 255.0) as u32;
        if weight == 255 {
            return self.color;
        }
        let mut color = 0xff00_0000;
        for shift in [0, 8, 16] {
            let target = (self.color >> shift) & 255;
            let channel =
                255_u32.wrapping_add(target.wrapping_sub(255).wrapping_mul(weight) >> 8) & 255;
            color |= channel << shift;
        }
        color
    }
}

fn truncate_word(value: f32) -> u32 {
    if value.is_finite()
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&f64::from(value))
    {
        (value as i64) as u32
    } else {
        0 // x87 integer-indefinite's low word.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environmental_tint_matches_original_initialization_and_model_update()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut cases = 0;
        for line in include_str!("../../tests/fixtures/environmental_tint_native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let words = line
                .split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()?;
            let mut tint = UnitModelTint::default();
            tint.apply(
                words[3],
                [
                    f32::from_bits(words[0]),
                    f32::from_bits(words[1]),
                    f32::from_bits(words[2]),
                    0.0,
                ],
            );
            assert_eq!(
                [tint.started, tint.color, tint.hold_ms, tint.fade_ms],
                words[3..7],
                "{line}"
            );
            let color = tint.sample(words[7], u32::MAX);
            assert_eq!(color, words[8], "{line}");
            assert_eq!(tint.started, words[9], "{line}");
            for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                let value = ((color >> shift) & 255) as f32 * f32::from_bits(0x3b808081);
                assert_eq!(value.to_bits(), words[10 + channel], "{line}");
            }
            cases += 1;
        }
        assert_eq!(cases, 216);
        Ok(())
    }
}
