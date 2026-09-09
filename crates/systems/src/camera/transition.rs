//! Camera mode, target, vehicle, and shake transitions.

use std::f32::consts::PI;

use super::{
    CameraSubjectHeight, CameraSubjectHeightSource, MountCameraGeometry, MountCameraHeightError,
    PlayerCameraHeightSample,
};

/// Marker changes no larger than this leave the current stock target intact.
const ANIMATED_MARKER_TOLERANCE: f32 = 0.05;

/// Default values registered for the two build-12340 camera CVars.
const CAMERA_HEIGHT_SMOOTH_SPEED: f32 = 1.2;
const FLYING_MOUNT_HEIGHT_SMOOTH_SPEED: f32 = 2.0;

/// `$CMA` halves transition duration while present and for three seconds after.
const ANIMATED_MARKER_DURATION_FACTOR: f32 = 0.5;
const ANIMATED_MARKER_FACTOR_HOLD_MS: f32 = 3_000.0;

/// One cosine-eased scalar transition matching `CGCamera`'s float path.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraHeightTransition {
    current: f32,
    start: f32,
    target: f32,
    started_ms: f32,
    duration_ms: f32,
    collision_started: Option<u32>,
}

impl CameraHeightTransition {
    const fn new(value: f32) -> Self {
        Self {
            current: value,
            start: value,
            target: value,
            started_ms: 0.0,
            duration_ms: 0.0,
            collision_started: None,
        }
    }

    fn advance(&mut self, now_ms: f32) {
        if self.collision_started.is_some() {
            return;
        }
        if self.duration_ms <= 0.0 {
            self.current = self.target;
            return;
        }
        let progress = ((now_ms - self.started_ms) / self.duration_ms).clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.current = self.target;
            self.duration_ms = 0.0;
            return;
        }
        let eased = (1.0 - (progress * PI).cos()) * 0.5;
        self.current = self.start + (self.target - self.start) * eased;
    }

    fn retarget(&mut self, target: f32, speed: f32, factor: f32, now_ms: f32) {
        self.advance(now_ms);
        if (self.target - target).abs() < 0.001 || (self.current - target).abs() < 0.001 {
            return;
        }
        self.start = self.current;
        self.target = target;
        self.started_ms = now_ms;
        self.duration_ms = factor * (target - self.current).abs() / speed * 1_000.0;
        self.collision_started = None;
    }

    fn obstructed(&mut self, height: f32, now_ms: u32) {
        if f64::from(self.current) - f64::from(height) > f64::from(0.111_111_11_f32) {
            self.current = height + 0.111_112_066_f32;
            self.start = self.current;
            self.collision_started = Some(now_ms);
            self.duration_ms = 2_000.0;
        }
    }

    fn advance_collision(&mut self, now_ms: u32) {
        let Some(started) = self.collision_started else {
            return;
        };
        if (f64::from(self.target) - f64::from(self.current)).abs() < f64::from(f32::EPSILON * 2.0)
        {
            self.target = self.current;
            self.collision_started = None;
            self.duration_ms = 0.0;
            return;
        }
        let elapsed = now_ms.wrapping_sub(started);
        if (elapsed as i32) < 0 {
            return;
        }
        let fraction = f64::from(elapsed) * f64::from(0.001_f32) / 2.0;
        self.current = if fraction < 1.0 {
            let fraction = f64::from(fraction as f32);
            ((1.0 - (fraction * f64::from(PI)).cos())
                * 0.5
                * (f64::from(self.target) - f64::from(self.start))
                + f64::from(self.start)) as f32
        } else {
            self.target
        };
    }
}

/// Stateful build-12340 ownership of ordinary and mounted camera heights.
///
/// The body-derived height remains the ordinary target. A mounted `$CMA`
/// declaration replaces that target using its live transformed height, while
/// `$CFM` is a separately smoothed obstruction height latched only once for a
/// mount generation. It is never added to the orbit pivot. No marker is
/// synthesized when the selected M2 omits it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCameraHeightState {
    base: CameraSubjectHeight,
    principal: CameraHeightTransition,
    flying_offset: CameraHeightTransition,
    mounted: bool,
    fixed_marker_latched: bool,
    last_animated_marker_ms: Option<f32>,
    source: CameraSubjectHeightSource,
}

impl PlayerCameraHeightState {
    /// Returns the requested principal height used by 605D60's collision queries.
    #[must_use]
    pub const fn target_height(&self) -> f32 {
        self.principal.target
    }

    /// Applies 606F90's primary anchor feedback to the existing height target.
    /// Recovery uses the wrapping client clock, independently of M2 pose time.
    ///
    /// # Errors
    /// Rejects non-finite resolved heights.
    pub fn obstructed(&mut self, height: f32, now_ms: u32) -> Result<(), MountCameraHeightError> {
        if !height.is_finite() {
            return Err(MountCameraHeightError::NonFiniteCollisionHeight);
        }
        self.principal.obstructed(height, now_ms);
        Ok(())
    }

    /// Advances 603D30's two-second collision recovery before camera queries.
    #[must_use]
    pub fn sample_collision(&mut self, now_ms: u32) -> PlayerCameraHeightSample {
        self.principal.advance_collision(now_ms);
        PlayerCameraHeightSample::new(
            CameraSubjectHeight::new(self.principal.current, self.source),
            self.flying_offset.current,
        )
    }

    /// Starts from the already validated body-model camera height.
    #[must_use]
    pub const fn new(base: CameraSubjectHeight) -> Self {
        Self {
            base,
            principal: CameraHeightTransition::new(base.value()),
            flying_offset: CameraHeightTransition::new(0.0),
            mounted: false,
            fixed_marker_latched: false,
            last_animated_marker_ms: None,
            source: base.source(),
        }
    }

    /// Starts another non-null mount generation at the current smoothed value.
    ///
    /// This resets only `$CFM`'s one-shot lookup ownership. The principal and
    /// flying-offset transitions remain continuous across an in-place mount
    /// replacement, as they do when the unit's mount model pointer changes.
    pub fn begin_mount_generation(&mut self, now_ms: f32) -> Result<(), MountCameraHeightError> {
        validate_time(now_ms)?;
        self.advance(now_ms);
        self.mounted = true;
        self.fixed_marker_latched = false;
        Ok(())
    }

    /// Begins a new mount generation without resetting the smoothed height.
    pub fn set_mounted(
        &mut self,
        mounted: bool,
        now_ms: f32,
    ) -> Result<(), MountCameraHeightError> {
        validate_time(now_ms)?;
        self.advance(now_ms);
        if self.mounted == mounted {
            return Ok(());
        }
        self.mounted = mounted;
        self.fixed_marker_latched = false;
        if !mounted {
            let factor = self.duration_factor(now_ms);
            self.principal.retarget(
                self.base.value(),
                CAMERA_HEIGHT_SMOOTH_SPEED,
                factor,
                now_ms,
            );
            self.flying_offset
                .retarget(0.0, FLYING_MOUNT_HEIGHT_SMOOTH_SPEED, factor, now_ms);
        }
        Ok(())
    }

    /// Applies the selected mount M2's current marker state.
    ///
    /// `$CMA` wins when present. `$CFM` is inspected only when `$CMA` is
    /// absent and only until the current mount generation has been latched.
    ///
    /// # Errors
    ///
    /// Returns [`MountCameraHeightError`] for non-finite time or marker data.
    pub fn update_mount(
        &mut self,
        geometry: MountCameraGeometry,
        now_ms: f32,
    ) -> Result<(), MountCameraHeightError> {
        validate_time(now_ms)?;
        let animated_height = geometry.animated_height();
        let fixed_height = geometry.fixed_height();
        if animated_height.is_some_and(|height| !height.is_finite()) {
            return Err(MountCameraHeightError::NonFiniteAnimatedHeight);
        }
        if fixed_height.is_some_and(|height| !height.is_finite()) {
            return Err(MountCameraHeightError::NonFiniteFixedHeight);
        }
        self.advance(now_ms);
        if !self.mounted {
            return Ok(());
        }
        if let Some(height) = animated_height {
            self.last_animated_marker_ms = Some(now_ms);
            self.source = CameraSubjectHeightSource::AnimatedMountMarker;
            self.flying_offset.retarget(
                0.0,
                FLYING_MOUNT_HEIGHT_SMOOTH_SPEED,
                ANIMATED_MARKER_DURATION_FACTOR,
                now_ms,
            );
            if (height - self.principal.target).abs() > ANIMATED_MARKER_TOLERANCE {
                self.principal.retarget(
                    height,
                    CAMERA_HEIGHT_SMOOTH_SPEED,
                    ANIMATED_MARKER_DURATION_FACTOR,
                    now_ms,
                );
            }
            return Ok(());
        }
        let factor = self.duration_factor(now_ms);
        self.principal.retarget(
            self.base.value(),
            CAMERA_HEIGHT_SMOOTH_SPEED,
            factor,
            now_ms,
        );
        if !self.fixed_marker_latched {
            self.fixed_marker_latched = true;
            let height = fixed_height.unwrap_or(0.0);
            self.source = self.base.source();
            self.flying_offset
                .retarget(height, FLYING_MOUNT_HEIGHT_SMOOTH_SPEED, factor, now_ms);
        }
        Ok(())
    }

    /// Samples the two independently smoothed camera-height inputs.
    ///
    /// # Errors
    ///
    /// Returns [`MountCameraHeightError::NonFiniteTime`] for invalid time.
    pub fn sample(
        &mut self,
        now_ms: f32,
    ) -> Result<PlayerCameraHeightSample, MountCameraHeightError> {
        validate_time(now_ms)?;
        self.advance(now_ms);
        Ok(PlayerCameraHeightSample::new(
            CameraSubjectHeight::new(self.principal.current, self.source),
            self.flying_offset.current,
        ))
    }

    fn advance(&mut self, now_ms: f32) {
        self.principal.advance(now_ms);
        self.flying_offset.advance(now_ms);
        if !self.mounted
            && self.principal.duration_ms <= 0.0
            && self.flying_offset.duration_ms <= 0.0
        {
            self.source = self.base.source();
        }
    }

    fn duration_factor(self, now_ms: f32) -> f32 {
        self.last_animated_marker_ms.map_or(1.0, |last_ms| {
            if now_ms - last_ms < ANIMATED_MARKER_FACTOR_HOLD_MS {
                ANIMATED_MARKER_DURATION_FACTOR
            } else {
                1.0
            }
        })
    }
}

fn validate_time(now_ms: f32) -> Result<(), MountCameraHeightError> {
    if now_ms.is_finite() {
        Ok(())
    } else {
        Err(MountCameraHeightError::NonFiniteTime)
    }
}

#[cfg(test)]
#[path = "../../tests/stock_seed/camera_height_recovery.rs"]
mod tests;
