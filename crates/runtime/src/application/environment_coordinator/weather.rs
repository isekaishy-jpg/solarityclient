//! Native 7846A0/784850 weather lighting anchors and independent transitions.

#[cfg(test)]
#[path = "../../../tests/application/weather_transition.rs"]
mod tests;

/// Weather lighting evolves independently of precipitation resource drainage.
#[derive(Clone, Debug)]
pub(super) struct WeatherTransition {
    kind: u32,
    target_grade: f32,
    previous_grade: f32,
    grade: f32,
    target_light: f32,
    previous_light: f32,
    light: f32,
    started: u32,
    weight: f32,
    previous_weight: f32,
    target_weight: f32,
    force_update: bool,
}

impl Default for WeatherTransition {
    fn default() -> Self {
        Self {
            kind: 0,
            target_grade: 0.,
            previous_grade: 0.,
            grade: 0.,
            target_light: 0.,
            previous_light: 0.,
            light: 0.,
            started: 0,
            weight: 1.,
            previous_weight: 1.,
            target_weight: 1.,
            force_update: false,
        }
    }
}

impl WeatherTransition {
    pub(super) fn receive(
        &mut self,
        kind: u32,
        grade: f32,
        instant: bool,
        mut weight: f32,
        now: u32,
    ) {
        if kind >= 4 {
            return;
        }
        self.previous_grade = self.target_grade;
        self.target_grade = grade.clamp(0., 1.);
        if self.kind == 0 {
            self.target_weight = weight;
        } else if kind == 0 {
            weight = self.target_weight;
        }
        self.previous_weight = self.target_weight;
        self.target_weight = weight;
        if instant {
            self.force_update = true;
            self.previous_grade = self.target_grade;
            self.grade = self.target_grade;
            self.previous_weight = weight;
            self.weight = weight;
        }
        self.target_light = self.target_grade.min(0.25);
        self.previous_light = self.previous_grade.min(0.25);
        self.started = now;
        self.kind = kind;
    }

    pub(super) fn sample(&mut self, now: u32) -> f32 {
        let changed = |current: f32, target: f32| {
            (f64::from(current) - f64::from(target)).abs() >= f64::from(f32::from_bits(0x34800000))
        };
        if self.force_update
            || changed(self.grade, self.target_grade)
            || changed(self.light, self.target_light)
            || changed(self.weight, self.target_weight)
        {
            self.interpolate(now);
        }
        ((self.light * self.weight) * 4.).min(1.)
    }

    fn interpolate(&mut self, now: u32) {
        let elapsed = f64::from(now.wrapping_sub(self.started)) * f64::from(0.001_f32);
        self.grade = transition(self.previous_grade, self.target_grade, elapsed, 1., 10.);
        self.light = transition(self.previous_light, self.target_light, elapsed, 4., 10.);
        self.weight = transition(self.previous_weight, self.target_weight, elapsed, 4., 5.);
        self.force_update = false;
    }
}

fn transition(from: f32, to: f32, elapsed: f64, scale: f64, duration: f64) -> f32 {
    let difference = f64::from(to) - f64::from(from);
    let divisor = (difference * scale + f64::from(0.001_f32)).abs() * duration;
    let fraction = (elapsed / divisor).clamp(0., 1.);
    (f64::from(from) + difference * fraction) as f32
}
