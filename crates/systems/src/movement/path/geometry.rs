//! Linear and Catmull-Rom paths from native vtable `009E2F28`.

use glam::Vec3;
use thiserror::Error;

const POSITION_BASIS: [[f64; 4]; 4] = [
    [-0.5, 1.0, -0.5, 0.0],
    [1.5, -2.5, 0.0, 1.0],
    [-1.5, 2.0, 0.5, 0.0],
    [0.5, -0.5, 0.0, 0.0],
];

/// Native path mode; nonzero native modes use the Catmull-Rom basis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementPathMode {
    /// Interpolate between the middle two controls of each four-point window.
    Linear,
    /// Evaluate all four controls using the stock Catmull-Rom basis.
    Smooth,
}

/// Geometry failure at the safe movement-owner admission boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MovementPathError {
    /// A native segment requires four controls, including endpoint controls.
    #[error("movement path has fewer than four control points")]
    MissingControls,
    /// Geometry must be finite before it can enter spatial state.
    #[error("movement path contains a nonfinite control point")]
    NonfinitePoint,
}

/// Evaluated position and the direction used by the unit orientation owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementPathSample {
    /// Position in the path's coordinate system.
    pub position: Vec3,
    /// Native normalized direction, or the supplied previous direction when
    /// a smooth segment has no usable tangent.
    pub direction: Vec3,
}

/// Retained controls and native per-segment length cache.
#[derive(Clone, Debug)]
pub struct MovementPath {
    nodes: Vec<Vec3>,
    mode: MovementPathMode,
    lengths: Vec<f32>,
    length: f32,
}

impl MovementPath {
    /// Builds the native `004C3830` length cache without losing endpoint controls.
    ///
    /// # Errors
    /// Rejects missing/nonfinite controls at the safe Rust admission boundary.
    /// Native assumes valid server geometry; these checks prevent out-of-bounds
    /// reads and nonfinite world placement without inventing a replacement path.
    pub fn new(nodes: Vec<Vec3>, mode: MovementPathMode) -> Result<Self, MovementPathError> {
        if nodes.len() < 4 {
            return Err(MovementPathError::MissingControls);
        }
        if nodes.iter().any(|point| !point.is_finite()) {
            return Err(MovementPathError::NonfinitePoint);
        }
        let lengths: Vec<_> = nodes
            .windows(4)
            .map(|controls| segment_length(controls, mode))
            .collect();
        let length = lengths.iter().map(|value| f64::from(*value)).sum::<f64>() as f32;
        Ok(Self {
            nodes,
            mode,
            lengths,
            length,
        })
    }

    /// Returns the native cached length in yards, before duration scaling.
    #[must_use]
    pub const fn length(&self) -> f32 {
        self.length
    }

    /// Native `004C3720` sums completed segments before a DBC control node.
    /// Both the approach control and first traversed point have distance zero.
    pub(crate) fn distance_to_control(&self, index: usize) -> f64 {
        self.lengths
            .iter()
            .take(index.saturating_sub(1))
            .map(|length| f64::from(*length))
            .sum()
    }

    /// `004C3920`/`004C42C0` return the raw derivative for transport banking,
    /// without the chord admission policy of the complete placement sampler.
    pub(crate) fn tangent(&self, fraction: f32) -> Vec3 {
        let (segment, parameter) = self.segment_parameter(fraction.clamp(0.0, 1.0));
        polynomial(&self.nodes[segment..segment + 4], parameter, true)
    }

    /// `0098C940` removes the initial approach control and closes the cycle
    /// with the old third-to-last control before rebuilding the length cache.
    pub(super) fn enter_cycle(&mut self) -> Result<(), MovementPathError> {
        let closing = self.nodes[self.nodes.len() - 3];
        let mut nodes = self.nodes[1..].to_vec();
        nodes[0] = closing;
        *self = Self::new(nodes, self.mode)?;
        Ok(())
    }

    /// Samples native `004C3980`/`004C43B0` at a normalized traversal fraction.
    /// The native wrapper clamps the fraction to zero through one.
    #[must_use]
    pub fn sample(&self, fraction: f32, previous_direction: Vec3) -> MovementPathSample {
        let fraction = if fraction >= 0.0 {
            fraction.min(1.0)
        } else {
            0.0
        };
        let (segment, parameter) = self.segment_parameter(fraction);
        let controls = &self.nodes[segment..segment + 4];
        let position = segment_position(controls, self.mode, parameter);
        let chord = normalize_above(controls[2] - controls[1], f32::EPSILON * 2.0);
        let direction = if self.mode == MovementPathMode::Linear {
            chord
        } else {
            let tangent = polynomial(controls, parameter, true);
            let squared = squared_length(tangent);
            if squared > 0.0001 {
                let tangent = scale(tangent, (1.0 / f64::from(squared).sqrt()) as f32);
                if dot(tangent, chord) >= 0.5 {
                    tangent
                } else {
                    chord
                }
            } else {
                previous_direction
            }
        };
        MovementPathSample {
            position,
            direction,
        }
    }

    /// `004C3BD0` apportions duration by cached segment length, with exact
    /// boundaries advancing to the following segment.
    fn segment_parameter(&self, fraction: f32) -> (usize, f32) {
        if self.lengths.len() == 1 {
            return (0, fraction);
        }
        let distance = f64::from(self.length) * f64::from(fraction);
        let mut start = 0.0;
        let mut segment = 0;
        while segment < self.lengths.len() - 1 {
            let end = start + f64::from(self.lengths[segment]);
            if end > distance {
                break;
            }
            start = end;
            segment += 1;
        }
        (
            segment,
            ((distance - start) / f64::from(self.lengths[segment])) as f32,
        )
    }
}

/// Native `004C40B0` uses chord length for linear paths and twenty sampled
/// chords for smooth paths (`004C3B10`), accumulating each smooth chord in f32.
fn segment_length(controls: &[Vec3], mode: MovementPathMode) -> f32 {
    if mode == MovementPathMode::Linear {
        let difference = controls[2].as_dvec3() - controls[1].as_dvec3();
        return difference.length() as f32;
    }
    let mut previous = polynomial(controls, 0.0, false);
    let mut parameter = 0.05_f32;
    let mut length = 0.0_f32;
    for _ in 0..20 {
        let point = polynomial(controls, parameter, false);
        let difference = point.as_dvec3() - previous.as_dvec3();
        length = (difference.length() + f64::from(length)) as f32;
        previous = point;
        parameter += 0.05_f32;
    }
    length
}

/// `004C3FD0` selects the linear or cubic position kernel.
fn segment_position(controls: &[Vec3], mode: MovementPathMode, parameter: f32) -> Vec3 {
    if mode == MovementPathMode::Smooth {
        return polynomial(controls, parameter, false);
    }
    let start = controls[1].as_dvec3();
    (start + (controls[2].as_dvec3() - start) * f64::from(parameter)).as_vec3()
}

/// `004C39D0`/`004C3A70` retain each coefficient in x87 precision and store
/// each accumulated vector component as float after each control point.
fn polynomial(controls: &[Vec3], parameter: f32, derivative: bool) -> Vec3 {
    let t = f64::from(parameter);
    let mut result = Vec3::ZERO;
    for (point, [a, b, c, d]) in controls.iter().zip(POSITION_BASIS) {
        let weight = if derivative {
            (3.0 * a * t + 2.0 * b) * t + c
        } else {
            ((a * t + b) * t + c) * t + d
        };
        result = (result.as_dvec3() + point.as_dvec3() * weight).as_vec3();
    }
    result
}

/// Keeps the native float store before the normalization threshold.
fn squared_length(value: Vec3) -> f32 {
    value.as_dvec3().length_squared() as f32
}

/// Normalizes only the vectors admitted by native `004C43B0`.
fn normalize_above(value: Vec3, threshold: f32) -> Vec3 {
    let squared = squared_length(value);
    if squared > threshold {
        scale(value, (1.0 / f64::from(squared).sqrt()) as f32)
    } else {
        value
    }
}

/// Component products have the same final float stores as the stock kernel.
fn scale(value: Vec3, factor: f32) -> Vec3 {
    (value.as_dvec3() * f64::from(factor)).as_vec3()
}

/// The direction admission dot product stays in extended precision.
fn dot(left: Vec3, right: Vec3) -> f64 {
    left.as_dvec3().dot(right.as_dvec3())
}
