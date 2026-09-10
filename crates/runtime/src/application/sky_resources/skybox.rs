//! Native three-slot selection and the per-path animation phase cache.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SkyboxSlot {
    pub(super) model: Option<usize>,
    pub(super) weight: f32,
    pub(super) flags: u32,
}

/// 7F09B0 checks readiness before suppressing the default sky compositor.
pub(super) fn default_sky(slots: &[SkyboxSlot; 4], mut ready: impl FnMut(usize) -> bool) -> bool {
    !slots.iter().enumerate().any(|(index, slot)| {
        (index == 3 || slot.flags == 0) && slot.weight > 0.99 && slot.model.is_some_and(&mut ready)
    })
}

/// Full global weight suppresses ordinary draws even before that model is ready.
pub(super) fn admits_slot(index: usize, global: SkyboxSlot) -> bool {
    if index == 3 {
        global.model.is_some() && global.weight > 0.
    } else {
        global.model.is_none() || global.weight < 1.
    }
}

/// 79A870 replaces slot zero and clears slot one, retaining the third DBC slot.
pub(super) fn replace_world_model(slots: &mut [SkyboxSlot; 3], model: Option<usize>, weight: f32) {
    slots[0] = SkyboxSlot {
        model,
        weight,
        flags: 0,
    };
    slots[1] = SkyboxSlot::default();
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
#[path = "../../../tests/application/skybox.rs"]
mod tests;
