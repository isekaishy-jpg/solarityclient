//! Server path preparation and current-position reconciliation at `0073C8E0`.

use glam::Vec3;
use solarity_ecs::WorldTransform;

use super::MovementPathError;

const POINT_TOLERANCE_SQUARED: f64 = f32::from_bits(0x3a4a_4588) as f64;

/// Decoded server points before native endpoint controls are constructed.
#[derive(Clone, Debug)]
pub enum MovementPathRequest {
    /// Ordinary path points, excluding the separately transmitted start.
    Move {
        /// Initial server position, used by the linear path branch.
        start: Vec3,
        /// Ordered intermediate/destination points in path coordinates.
        points: Vec<Vec3>,
        /// Exact spline flags.
        flags: u32,
        /// Authored traversal time in milliseconds.
        duration_ms: u32,
    },
    /// Short stop form, with a server-supplied final point.
    Stop {
        /// Desired final location in path coordinates.
        destination: Vec3,
    },
}

/// Native admission result before a live spline is allocated.
#[derive(Clone, Debug)]
pub enum PreparedMovementPath {
    /// Enough travel remains to install an interpolated path.
    Spline {
        /// Complete native endpoint and traversal controls.
        controls: Vec<Vec3>,
        /// Destination sampled by `006EB680` from the completed controls.
        destination: Vec3,
        /// Effective time after the native run-speed cap, in milliseconds.
        duration_ms: u32,
        /// Exact flags, or 0x1000 for a short stop correction.
        flags: u32,
    },
    /// Native `006F11B0` places the point without an active path.
    Place {
        /// Final location supplied to the placement owner.
        destination: Vec3,
        /// Original movement flags, or zero for a stop within its tolerance.
        flags: u32,
    },
}

impl MovementPathRequest {
    /// Builds native controls using the current unit transform and run speed.
    /// `stop_distance_tolerance` is the `pathDistTol` CVar (stock default one).
    ///
    /// # Errors
    /// Rejects nonfinite points or insufficient controls at the safe boundary.
    pub fn prepare(
        self,
        current: WorldTransform,
        run_speed: f32,
        stop_distance_tolerance: f32,
    ) -> Result<PreparedMovementPath, MovementPathError> {
        if !current.position().is_finite()
            || !current.orientation().is_finite()
            || !run_speed.is_finite()
            || !stop_distance_tolerance.is_finite()
        {
            return Err(MovementPathError::NonfinitePoint);
        }
        let (controls, destination, flags, duration_ms) = match self {
            Self::Stop { destination } => {
                if !destination.is_finite() {
                    return Err(MovementPathError::NonfinitePoint);
                }
                if distance_squared(current.position(), destination)
                    < f64::from(stop_distance_tolerance).powi(2)
                {
                    return Ok(PreparedMovementPath::Place {
                        destination,
                        flags: 0,
                    });
                }
                let preceding = preceding_control(current);
                (
                    vec![preceding, current.position(), destination, destination],
                    destination,
                    0x1000,
                    0,
                )
            }
            Self::Move {
                start,
                points,
                flags,
                duration_ms,
            } => {
                if !start.is_finite() || points.iter().any(|point| !point.is_finite()) {
                    return Err(MovementPathError::NonfinitePoint);
                }
                let Some(&destination) = points.last() else {
                    return Err(MovementPathError::MissingControls);
                };
                let controls = if flags & 0x42000 == 0 {
                    let mut traversal = Vec::with_capacity(points.len() + 2);
                    traversal.push(start);
                    if points.len() > 1
                        || distance_squared(start, destination) > POINT_TOLERANCE_SQUARED
                    {
                        traversal.extend(points);
                    }
                    reconcile_linear_start(&mut traversal, current.position());
                    if traversal.len() < 2 {
                        return Ok(PreparedMovementPath::Place { destination, flags });
                    }
                    let preceding =
                        (traversal[0].as_dvec3() * 2.0 - traversal[1].as_dvec3()).as_vec3();
                    let mut controls = Vec::with_capacity(traversal.len() + 2);
                    controls.push(preceding);
                    controls.extend(traversal);
                    controls.push(destination);
                    controls
                } else {
                    let mut controls = Vec::with_capacity(points.len() + 4);
                    controls.extend([preceding_control(current), current.position()]);
                    if distance_squared(current.position(), points[0]) >= POINT_TOLERANCE_SQUARED {
                        controls.push(points[0]);
                    }
                    controls.extend_from_slice(&points[1..]);
                    if flags & 0x80000 == 0 {
                        controls.push(destination);
                    } else {
                        let Some(first) = controls.get(2).copied() else {
                            return Err(MovementPathError::MissingControls);
                        };
                        let Some(second) = controls.get(3).copied() else {
                            return Err(MovementPathError::MissingControls);
                        };
                        controls.extend([first, second]);
                    }
                    controls
                };
                (controls, destination, flags, duration_ms)
            }
        };
        if controls.len() < 4 {
            return Ok(PreparedMovementPath::Place { destination, flags });
        }
        // 0073CF45 sums traversal chords backwards in extended precision.
        // It intentionally does not use the smooth curve's later length cache.
        let length: f64 = controls[1..controls.len() - 1]
            .windows(2)
            .rev()
            .map(|pair| distance_squared(pair[0], pair[1]).sqrt())
            .sum();
        if length <= f64::from(1.0_f32 / 6.0) {
            return Ok(PreparedMovementPath::Place { destination, flags });
        }
        let maximum_speed = if flags & 0x42000 != 0 {
            50.0
        } else {
            (f64::from(run_speed) * 4.0).max(28.0)
        };
        let speed = if duration_ms == 0 {
            maximum_speed
        } else {
            maximum_speed.min(length / (f64::from(duration_ms) * f64::from(0.001_f32)))
        };
        if speed <= f64::from(f32::from_bits(0x3580_0000)) {
            return Ok(PreparedMovementPath::Place { destination, flags });
        }
        // Native stores milliseconds to float before its nearest-even FISTP.
        let duration_ms = ((length / speed * 1000.0) as f32)
            .round_ties_even()
            .max(1.0) as u32;
        let destination = controls[controls.len() - 2];
        Ok(PreparedMovementPath::Spline {
            controls,
            destination,
            duration_ms,
            flags,
        })
    }
}

/// The smooth/stop branch uses one unit behind the current facing as a control.
fn preceding_control(current: WorldTransform) -> Vec3 {
    let direction = Vec3::new(
        current.orientation().cos(),
        current.orientation().sin(),
        0.0,
    );
    (current.position().as_dvec3() - direction.as_dvec3()).as_vec3()
}

/// Stock `007180C0` selects the remaining linear path from the unit's current
/// location. Segment planes trim travelled points and preserve lateral offsets.
fn reconcile_linear_start(points: &mut Vec<Vec3>, current: Vec3) {
    if points.len() == 1 {
        if distance_squared(points[0], current) >= POINT_TOLERANCE_SQUARED {
            points.insert(0, current);
        } else {
            points.clear();
        }
        return;
    }
    let mut before = 0;
    let mut after = 0;
    for segment in points.windows(2) {
        let (start, end) = segment_planes(segment[0], segment[1], current, false);
        if start >= 0.0 && end >= 0.0 {
            before += 1;
        } else if start <= 0.0 && end <= 0.0 {
            after += 1;
        } else {
            break;
        }
    }
    if before == points.len() - 1 {
        prepend_if_distant_xy(points, current);
        return;
    }
    if after == points.len() - 1 {
        let destination = points[points.len() - 1];
        points.clear();
        if distance_squared(destination, current) >= POINT_TOLERANCE_SQUARED {
            points.extend([current, destination]);
        }
        return;
    }
    for index in 0..points.len() - 1 {
        let (start, end) = segment_planes(points[index], points[index + 1], current, true);
        if start <= 0.0 && end >= 0.0 {
            let fraction = start / (start - end);
            let projected = (points[index].as_dvec3()
                + (points[index + 1].as_dvec3() - points[index].as_dvec3()) * fraction)
                .as_vec3();
            if distance_squared(projected, points[index + 1]) >= POINT_TOLERANCE_SQUARED {
                points[index] = projected;
                points.drain(..index);
            } else if distance_squared(points[index + 1], current) >= POINT_TOLERANCE_SQUARED {
                points[index] = current;
                points.drain(..index);
            } else if index + 1 == points.len() - 1 {
                points.clear();
            } else {
                points.drain(..index + 1);
            }
            return;
        }
        if start >= 0.0 && end >= 0.0 {
            points.drain(..index);
            prepend_if_distant_xy(points, current);
            return;
        }
    }
}

/// Native evaluates dot products against a plane through the current point.
fn segment_planes(start: Vec3, end: Vec3, current: Vec3, normalize: bool) -> (f64, f64) {
    let mut direction = end.as_dvec3() - start.as_dvec3();
    if normalize {
        direction /= direction.length();
    }
    let offset = -direction.dot(current.as_dvec3());
    (
        direction.dot(start.as_dvec3()) + offset,
        direction.dot(end.as_dvec3()) + offset,
    )
}

/// The stock prepend gate uses XY distance one, not the smaller 3D tolerance.
fn prepend_if_distant_xy(points: &mut Vec<Vec3>, current: Vec3) {
    if (points[0].as_dvec3() - current.as_dvec3())
        .truncate()
        .length_squared()
        >= 1.0
    {
        points.insert(0, current);
    }
}

/// Differences remain in x87 precision until the caller stores a final point.
fn distance_squared(left: Vec3, right: Vec3) -> f64 {
    (left.as_dvec3() - right.as_dvec3()).length_squared()
}
