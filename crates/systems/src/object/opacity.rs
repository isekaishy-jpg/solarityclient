//! Build-12340 CObject opacity transitions and detached scene retirement.

/// Byte-valued model opacity owned by one gameplay-object lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityOpacity {
    started_at: u32,
    duration_ms: u32,
    current: u8,
    from: u8,
    target: u8,
    multiplier: u8,
}

impl Default for EntityOpacity {
    fn default() -> Self {
        Self {
            started_at: 0,
            duration_ms: 0,
            current: 0,
            from: 0,
            target: 0,
            multiplier: 255,
        }
    }
}

impl EntityOpacity {
    /// 716650 selects the entry duration before publishing a newly admitted unit model.
    /// `parent_transitioning` is absent when the transport unit cannot be resolved;
    /// the signed seat attachment is absent when the vehicle/seat lookup fails.
    #[must_use]
    pub fn unit_entry_duration(
        primary_flags: u32,
        secondary_flags: u32,
        bytes1: u32,
        transport_guid: u64,
        parent_transitioning: Option<bool>,
        seat_attachment_id: Option<i32>,
    ) -> u32 {
        if primary_flags & 2 != 0 || secondary_flags & 0x20 != 0 || bytes1 & 0x0002_0000 != 0 {
            return 0;
        }
        let high = (transport_guid >> 32) as u32;
        let unit_transport = high & 0xf0f0_0000 == 0xf050_0000
            || (high & 0xf000_0000 == 0 && (transport_guid as u32 != 0 || high & 0xf07f_ffff != 0));
        if unit_transport
            && (parent_transitioning.is_none()
                || (parent_transitioning == Some(true)
                    && seat_attachment_id.is_none_or(|attachment| attachment >= 0)))
        {
            0
        } else {
            1000
        }
    }

    /// 744030 quantizes the target before comparing it with the saved current byte.
    pub fn select(&mut self, target: f32, duration_ms: u32, now: u32) {
        let target = opacity_byte(target);
        self.target = target;
        if target == self.current {
            self.duration_ms = 0;
            return;
        }
        self.started_at = now;
        self.duration_ms = duration_ms;
        self.from = self.current;
        if duration_ms == 0 {
            self.current = target;
        }
    }

    /// 743E10/71AC30 update only when the selected model exists, even offscreen.
    pub fn advance(&mut self, now: u32) {
        if self.duration_ms == 0 {
            return;
        }
        let elapsed = now.wrapping_sub(self.started_at);
        if elapsed.wrapping_sub(self.duration_ms) as i32 >= 0 {
            self.current = self.target;
            self.duration_ms = 0;
        } else {
            let change = i32::from(self.target) - i32::from(self.from);
            let change = change
                .wrapping_mul(elapsed as i32)
                .wrapping_div(self.duration_ms as i32);
            self.current = self.from.wrapping_add(change as u8);
        }
    }

    /// Separate camera/vehicle opacity supplied by the Unit_C multiplier callback.
    pub fn set_multiplier(&mut self, opacity: f32) {
        self.multiplier = opacity_byte(opacity);
    }

    /// Model creation can explicitly finish the current transition without retiming it.
    pub fn finish(&mut self) {
        self.duration_ms = 0;
        self.current = self.target;
    }

    /// Returns whether the timed transition remains active.
    #[must_use]
    pub const fn transitioning(self) -> bool {
        self.duration_ms != 0
    }

    /// Returns the scalar published to the model, including both native byte factors.
    #[must_use]
    pub fn opacity(self) -> f32 {
        // 00A34C10 is the stored float reciprocal of 255 squared. The native
        // x87 multiplication keeps the integer product exact until publication.
        const SCALE: f32 = f32::from_bits(0x3781_0182);
        (f64::from(self.current) * f64::from(self.multiplier) * f64::from(SCALE)) as f32
    }

    /// 743D50 transfers the primary byte alone to the detached scene owner.
    #[must_use]
    pub fn retirement_opacity(self) -> f32 {
        (f64::from(self.current) * f64::from(f32::from_bits(0x3b80_8081))) as f32
    }
}

fn opacity_byte(value: f32) -> u8 {
    let rounded = (value * 255.0).round_ties_even();
    // FISTP produces the integer-indefinite word for nonfinite/out-of-range
    // input. The native setter then keeps its low byte, including ordinary wrap.
    if !rounded.is_finite() || !(-2_147_483_648.0..2_147_483_648.0).contains(&rounded) {
        0
    } else {
        rounded as i32 as u8
    }
}

/// A detached, already admitted model continues rendering for at most two seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityRetirement {
    started_at: u32,
    initial_opacity: f32,
}

impl EntityRetirement {
    /// Stores the primary opacity supplied by 783630 when it detaches the scene entry.
    #[must_use]
    pub const fn new(started_at: u32, initial_opacity: f32) -> Self {
        Self {
            started_at,
            initial_opacity,
        }
    }

    /// 782F20 retains the exact 2000-ms endpoint and retires an unready model immediately.
    #[must_use]
    pub fn sample(self, now: u32, model_ready: bool) -> Option<f32> {
        let elapsed = now.wrapping_sub(self.started_at) as i32;
        if !model_ready || elapsed > 2000 {
            return None;
        }
        let value =
            (1.0 - f64::from(elapsed) * f64::from(0.0005_f32)) * f64::from(self.initial_opacity);
        Some(if value < 0.0 {
            0.0
        } else if value > 1.0 {
            1.0
        } else {
            ((3.0 - (value + value)) * value * value) as f32
        })
    }
}

#[cfg(test)]
#[path = "../../tests/object/opacity.rs"]
mod tests;
