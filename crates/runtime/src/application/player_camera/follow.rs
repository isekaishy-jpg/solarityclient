//! Relative ordinary-player follow requests (602760) and timed axis lanes.

#[derive(Clone, Copy)]
pub(in crate::application) struct FollowSettings {
    pub style: usize,
    pub enabled: [bool; 2],
    pub bounds: [[f32; 2]; 2],
    pub speed: [f32; 2],
    pub time_bounds: [f32; 2],
    pub conditions: [[[f32; 2]; 7]; 5],
    pub axes: [[[f32; 2]; 2]; 5],
}

impl FollowSettings {
    pub fn read(number: &impl Fn(&str) -> Option<f32>) -> Self {
        let mut settings = Self {
            style: number("camerasmoothstyle").unwrap_or(4.) as usize,
            ..Self::default()
        };
        for (index, axis) in ["pitch", "yaw"].into_iter().enumerate() {
            settings.enabled[index] = number(&format!("camerasmooth{axis}")).unwrap_or(1.) != 0.;
            settings.speed[index] =
                number(&format!("camera{axis}smoothspeed")).unwrap_or(settings.speed[index]);
            for (bound, suffix) in ["min", "max"].into_iter().enumerate() {
                if let Some(value) = number(&format!("camera{axis}smooth{suffix}")) {
                    settings.bounds[index][bound] =
                        (f64::from(value) * f64::from(0.017_453_292_f32)) as f32;
                }
            }
        }
        for (bound, suffix) in ["min", "max"].into_iter().enumerate() {
            settings.time_bounds[bound] =
                number(&format!("camerasmoothtime{suffix}")).unwrap_or(settings.time_bounds[bound]);
        }
        for (style_index, style) in ["never", "smart", "always", "spline", "smarter"]
            .into_iter()
            .enumerate()
        {
            for (param_index, param) in ["delay", "factor"].into_iter().enumerate() {
                for (condition_index, condition) in
                    ["idle", "stop", "track", "move", "strafe", "turn", "fear"]
                        .into_iter()
                        .enumerate()
                {
                    if let Some(value) = number(&format!("camerasmooth{style}{condition}{param}")) {
                        settings.conditions[style_index][condition_index][param_index] = value;
                    }
                }
                for (axis_index, axis) in ["pitch", "yaw"].into_iter().enumerate() {
                    if let Some(value) =
                        number(&format!("camerasmoothviewdata{style}{axis}{param}"))
                    {
                        settings.axes[style_index][axis_index][param_index] = value;
                    }
                }
            }
        }
        settings
    }
}

impl Default for FollowSettings {
    fn default() -> Self {
        let smart = [
            [0., 0.],
            [0., 0.],
            [0.4, 10.],
            [0., 1.],
            [0., 1.],
            [0., 1.],
            [0.4, 10.],
        ];
        Self {
            style: 4,
            enabled: [true; 2],
            bounds: [[0., 30. * 0.017_453_292_f32], [0., 0.]],
            speed: [45., 180.],
            time_bounds: [0.1, 2.],
            conditions: [
                [[0., 0.]; 7],
                smart,
                [[0., 1.]; 7],
                [
                    [0., 4.],
                    [0., 4.],
                    [0., 4.],
                    [0., 1.],
                    [0., 1.],
                    [0., 1.],
                    [0., 4.],
                ],
                smart,
            ],
            axes: [
                [[0., 0.]; 2],
                [[0., 0.], [0., 1.]],
                [[0., 1.]; 2],
                [[0., 1.]; 2],
                [[0., 1.]; 2],
            ],
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct FollowAngle {
    pub current: f32,
    pub active: bool,
    start: u32,
    duration: f32,
    goal: f32,
    anchor: f32,
    factor: f32,
    delay: f32,
}

impl FollowAngle {
    pub fn new(current: f32) -> Self {
        Self {
            current,
            active: false,
            start: 0,
            duration: 0.,
            goal: current,
            anchor: 0.,
            factor: 0.,
            delay: 0.,
        }
    }

    pub fn cancel(&mut self) {
        self.active = false;
        self.goal = self.current;
        self.start = 0;
        self.duration = 0.;
    }

    fn request(&mut self, goal: f32, delay: f32, factor: f32, speed: f32, time: u32) -> bool {
        let current = nearest(goal, self.current);
        self.current = current as f32;
        let close = |a: f32, b: f32| (f64::from(a) - f64::from(b)).abs() < f64::from(0.001_f32);
        if self.active
            && close(self.goal, goal)
            && close(self.delay, delay)
            && close(self.factor, factor)
        {
            return true;
        }
        if (current - f64::from(goal)).abs() < f64::from(0.001_f32) {
            return false;
        }
        self.delay = delay;
        self.factor = factor;
        let duration = ((f64::from(goal) - current).abs()
            / (f64::from(speed) * f64::from(0.017_453_292_f32))
            * f64::from(factor)) as f32;
        // 5FEB60 repeats normalization after the caller's float store.
        let current = nearest(goal, self.current);
        self.current = current as f32;
        if (current - f64::from(goal)).abs() < f64::from(0.001_f32) {
            return false;
        }
        self.active = true;
        self.anchor = self.current;
        self.goal = goal;
        self.duration = duration;
        self.start = time.wrapping_sub((f64::from(delay) * -1000.) as i64 as u32);
        true
    }

    pub fn sample(&mut self, time: u32) {
        if (f64::from(self.goal) - f64::from(self.current)).abs() < f64::from(f32::EPSILON * 2.) {
            self.cancel();
            return;
        }
        let elapsed = time.wrapping_sub(self.start);
        if !self.active || (elapsed as i32) < 0 {
            return;
        }
        let t = f64::from(elapsed) * f64::from(0.001_f32) / f64::from(self.duration);
        self.current = if t < 1. {
            // The fraction is stored to float before entering 8CA080.
            let t = f64::from(t as f32);
            ((1. - (t * f64::from(std::f32::consts::PI)).cos())
                * 0.5
                * (f64::from(self.goal) - f64::from(self.anchor))
                + f64::from(self.anchor)) as f32
        } else {
            self.goal
        };
    }
}

fn nearest(goal: f32, current: f32) -> f64 {
    let goal = f64::from(goal);
    let mut current = f64::from(current);
    let pi = f64::from(std::f32::consts::PI);
    let tau = f64::from(std::f32::consts::TAU);
    while goal - current < -pi {
        current -= tau;
    }
    while goal - current > pi {
        current += tau;
    }
    current
}

pub(super) fn idle(held: u32) -> bool {
    held & 0x1030 == 0
        && held & 0xc0 == 0
        && (held & 0x0200_0001 == 0 || held & 0x300 == 0)
        && (held & 0x300 == 0 || held & 0x0200_0001 != 0)
        && held & 0x01e0_0000 == 0
}

/// Highest native condition wins. Tracking and fear are supplied by their
/// separate camera owners; ordinary movement uses Idle/Stop/Move/Strafe/Turn.
pub(super) fn request(
    angles: &mut [FollowAngle; 2],
    flags: u32,
    held: u32,
    stopped: bool,
    time: u32,
    settings: &FollowSettings,
    swimming_or_flying: bool,
) {
    if flags & (1 | 0x20 | 0x8000) != 0 || settings.style >= 5 {
        return;
    }
    let turning = held & 0x300 != 0 || held & 0x0200_0001 != 0;
    let strafing = held & 0xc0 != 0 || (held & 0x0200_0001 != 0 && held & 0x300 != 0);
    let moving = held & 0x1030 != 0 || held & 3 == 3;
    let condition = if turning {
        5
    } else if strafing {
        4
    } else if moving {
        3
    } else if stopped {
        1
    } else if idle(held) {
        0
    } else {
        return;
    };
    let bounded = |v: f32| {
        if v < 0. {
            0.
        } else if v >= 100. {
            99.
        } else {
            v
        }
    };
    let condition = settings.conditions[settings.style][condition].map(bounded);
    let mut admitted = [false; 2];
    let mut duration = 0_f32;
    for axis in 0..2 {
        let angle = &mut angles[axis];
        let [min, max] = settings.bounds[axis];
        if !settings.enabled[axis]
            || (axis == 0 && swimming_or_flying)
            || (min <= angle.current && angle.current <= max)
        {
            continue;
        }
        let [delay, factor] = settings.axes[settings.style][axis];
        let delay = bounded(delay + condition[0]);
        let factor = bounded(factor * condition[1]);
        if factor == 0. {
            angle.cancel();
            continue;
        }
        let goal = if angle.current > max { max } else { min };
        admitted[axis] = angle.request(goal, delay, factor, settings.speed[axis], time);
        if admitted[axis] {
            duration = duration.max(angle.duration);
        }
    }
    duration = duration
        .max(settings.time_bounds[0])
        .min(settings.time_bounds[1]);
    for axis in 0..2 {
        if admitted[axis] {
            angles[axis].duration = duration;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_histories_match_original_policy_and_interpolation()
    -> Result<(), Box<dyn std::error::Error>> {
        for (index, line) in include_str!("../../../tests/fixtures/camera-follow-native.txt")
            .lines()
            .skip(1)
            .enumerate()
        {
            let words = line
                .split_whitespace()
                .filter(|word| *word != "|")
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()?;
            let mut settings = FollowSettings {
                style: words[0] as usize,
                ..Default::default()
            };
            if words[6] != 0 {
                settings.conditions = [[[0.125, 2.5]; 7]; 5];
            }
            let mut angles = [
                FollowAngle::new(f32::from_bits(words[3])),
                FollowAngle::new(f32::from_bits(words[2])),
            ];
            for step in words[7..].as_chunks::<17>().0 {
                if step[0] == 0 {
                    request(
                        &mut angles,
                        words[4],
                        words[1],
                        false,
                        step[1],
                        &settings,
                        false,
                    );
                } else {
                    angles[0].sample(step[1]);
                    if words[4] & 1 == 0 {
                        angles[1].sample(step[1]);
                    }
                }
                let mut actual = vec![
                    words[4]
                        | if angles[0].active { 0x0200_0000 } else { 0 }
                        | if angles[1].active { 0x0100_0000 } else { 0 },
                    angles[1].current.to_bits(),
                    angles[0].current.to_bits(),
                ];
                for angle in angles {
                    actual.extend([
                        angle.start,
                        angle.duration.to_bits(),
                        angle.goal.to_bits(),
                        angle.anchor.to_bits(),
                        angle.factor.to_bits(),
                        angle.delay.to_bits(),
                    ]);
                }
                assert_eq!(
                    actual,
                    step[2..],
                    "history {index}, kind {} time {}",
                    step[0],
                    step[1]
                );
            }
        }
        Ok(())
    }
}
