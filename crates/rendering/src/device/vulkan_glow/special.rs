//! Retained native screen-filter selection, seed row and three-second ramp.

/// One presentation sample for the native procedural screen filter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSpecialFrame {
    pub(super) color: u32,
    pub(super) decay: f32,
    pub(super) desaturation: f32,
    pub(super) whitening: f32,
    pub(super) seed_row: u32,
    pub(super) clear_history: bool,
}

impl WorldSpecialFrame {
    /// Returns the native per-draw desaturation constant before shader saturation.
    #[must_use]
    pub const fn desaturation(self) -> f32 {
        self.desaturation
    }
}

/// Process-owned screen-filter parameters and animation state.
#[derive(Debug)]
pub struct WorldSpecialState {
    color: u32,
    decay: f32,
    strength: f32,
    countdown: f32,
    seed_row: u32,
    clear_history: bool,
}

impl Default for WorldSpecialState {
    fn default() -> Self {
        Self {
            color: 0,
            decay: 0.,
            strength: 0.,
            countdown: 0.,
            seed_row: 0,
            clear_history: true,
        }
    }
}

impl WorldSpecialState {
    /// Applies 7E9010's signed integer conversions and activation countdown.
    /// Reselection clears image history but preserves the advancing noise row.
    pub fn select(&mut self, parameters: [u32; 4]) {
        self.color = parameters[0];
        self.decay = (f64::from(parameters[1] as i32) * f64::from(1_f32 / 255.)) as f32;
        self.strength = (f64::from(parameters[2] as i32) * f64::from(0.01_f32)) as f32;
        self.countdown = 3.;
        self.clear_history = true;
    }

    /// Samples 7E9670 before decrementing the countdown and advances 7E92A0's
    /// row once per presented effect frame, including a zero-delta frame.
    #[must_use]
    pub fn advance(&mut self, delta_seconds: f32) -> WorldSpecialFrame {
        let factor = if self.countdown > 0. {
            // The original keeps this subtraction and both products in x87.
            let factor = 1. - f64::from(self.countdown) * f64::from(1_f32 / 3.);
            self.countdown = (self.countdown - delta_seconds).max(0.);
            factor
        } else {
            1.
        };
        let frame = WorldSpecialFrame {
            color: self.color,
            decay: self.decay,
            desaturation: (f64::from(self.strength) * factor) as f32,
            whitening: (f64::from(0.1_f32) * factor) as f32,
            seed_row: self.seed_row,
            clear_history: std::mem::take(&mut self.clear_history),
        };
        self.seed_row = if self.seed_row >= 256 {
            0
        } else {
            self.seed_row + 1
        };
        frame
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/world_special.rs"]
mod tests;
