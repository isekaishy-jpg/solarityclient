//! Native three-slot selection and the per-path animation phase cache.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SkyboxSlot {
    pub(super) model: Option<usize>,
    pub(super) weight: f32,
    pub(super) flags: u32,
}

/// 7F3230 resolves requests in palette order, retaining native trailing slots.
pub(super) fn select_slots<E>(
    input: [(u32, f32); 3],
    mut resolve: impl FnMut(u32) -> Result<Option<(Option<usize>, u32)>, E>,
) -> Result<[SkyboxSlot; 3], E> {
    let mut slots = [SkyboxSlot::default(); 3];
    let mut count = 0;
    for (id, weight) in input {
        if id == 0 || weight <= 0. || weight.is_nan() {
            continue;
        }
        let Some((model, flags)) = resolve(id)? else {
            continue;
        };
        let flags = flags & 2;
        if weight > 0.99 && count > 0 && flags == 0 {
            count = 0;
        }
        slots[count] = SkyboxSlot {
            model,
            weight,
            flags,
        };
        count += 1;
    }
    if count < slots.len() {
        slots[count].weight = 0.;
    }
    Ok(slots)
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SkyboxPhase {
    pub(super) duration: u32,
    pub(super) last_minute: i32,
    pub(super) flags: u32,
}

impl SkyboxPhase {
    /// 7ECF20 reads the current primary's scene span once and remembers each
    /// sampled whole minute, even when a one-minute step does not seek.
    pub(super) fn update(&mut self, ready: bool, span: u32, minute: i32) -> Option<(i32, f32)> {
        if !ready {
            return None;
        }
        if self.duration == 0 {
            self.duration = span;
        }
        if self.duration == 0 {
            return None;
        }
        let change = minute.wrapping_sub(self.last_minute).wrapping_abs();
        let request = if change >= 2 && self.flags & 1 != 0 {
            let duration = f64::from(self.duration);
            let offset = native_phase_offset(minute, self.duration);
            let speed = (duration * f64::from(1.157_407_4e-8_f32)) as f32;
            Some((offset, speed))
        } else {
            None
        };
        self.last_minute = minute;
        request
    }
}

/// Preserve the x87 product's 64-bit significand before its truncating FISTP.
/// The first multiply (signed minute by the f32 constant) is exact; its second
/// multiply can exceed f64 precision for large unsigned animation spans.
fn native_phase_offset(minute: i32, duration: u32) -> i32 {
    let product = u128::from(minute.unsigned_abs()) * u128::from(duration) * 11_930_465;
    let shift = (128 - product.leading_zeros()).saturating_sub(64);
    let rounded = if shift == 0 {
        product
    } else {
        let mantissa = product >> shift;
        let remainder = product & ((1 << shift) - 1);
        let half = 1 << (shift - 1);
        (mantissa + u128::from(remainder > half || (remainder == half && mantissa & 1 != 0)))
            << shift
    };
    let value = (rounded >> 34) as u32;
    if minute < 0 {
        value.wrapping_neg() as i32
    } else {
        value as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_skybox_requests_and_slots() -> Result<(), Box<dyn std::error::Error>> {
        let mut phase = SkyboxPhase::default();
        for (line_index, line) in include_str!("../../../tests/fixtures/world_skybox_native.txt")
            .lines()
            .enumerate()
        {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let hex = |i: usize| u32::from_str_radix(fields[i], 16);
            match fields.first().copied() {
                Some("reset") => phase = SkyboxPhase::default(),
                Some("phase") => {
                    phase.flags = fields[2].parse()?;
                    let request =
                        phase.update(fields[3] == "1", fields[1].parse()?, fields[4].parse()?);
                    assert_eq!(
                        phase.duration,
                        fields[5].parse::<u32>()?,
                        "line {line_index}"
                    );
                    assert_eq!(phase.last_minute as u32, hex(6)?, "line {line_index}");
                    if fields[7] == "-" {
                        assert!(request.is_none(), "line {line_index}");
                    } else {
                        let (offset, speed) = request.ok_or("missing native skybox seek")?;
                        assert_eq!(
                            [u32::MAX, 0, u32::MAX, offset as u32, speed.to_bits(), 1, 1],
                            [
                                hex(7)?,
                                hex(8)?,
                                hex(9)?,
                                hex(10)?,
                                hex(11)?,
                                hex(12)?,
                                hex(13)?
                            ],
                            "line {line_index}"
                        );
                    }
                }
                Some("slots") => {
                    let inputs = [
                        (fields[1].parse()?, f32::from_bits(hex(4)?)),
                        (fields[2].parse()?, f32::from_bits(hex(5)?)),
                        (fields[3].parse()?, f32::from_bits(hex(6)?)),
                    ];
                    let slots = select_slots::<std::convert::Infallible>(inputs, |id| {
                        Ok(match id {
                            1 => Some((Some(100), 0)),
                            2 => Some((Some(101), 2)),
                            3 => Some((Some(102), 1)),
                            4 => Some((None, 0)),
                            _ => None,
                        })
                    })?;
                    for (i, slot) in slots.into_iter().enumerate() {
                        assert_eq!(
                            slot.model.unwrap_or(0) as u32,
                            hex(7 + i)?,
                            "line {line_index}"
                        );
                        assert_eq!(slot.weight.to_bits(), hex(10 + i)?, "line {line_index}");
                        assert_eq!(slot.flags, hex(13 + i)?, "line {line_index}");
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
