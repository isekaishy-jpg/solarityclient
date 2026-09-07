//! TaxiPathNode sections, stop/event clocks, and retained smooth route geometry.

use glam::Vec3;
use thiserror::Error;

use super::physics::TransportRoutePhysics;
use super::timing::{LegProfile, duration_ms, sample_leg};
use crate::movement::path::{MovementPath, MovementPathError, MovementPathMode};

/// Ordered TaxiPathNode fields consumed by native `007F90F0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportRouteNode {
    /// Map containing this control point.
    pub map_id: u32,
    /// Absolute world position in yards.
    pub position: Vec3,
    /// Bit zero splits after this node; bit one marks a stop.
    pub flags: u32,
    /// Station dwell duration; ignored unless flag two is set.
    pub delay_seconds: u32,
    /// Arrival callback ID, or zero for no callback.
    pub arrival_event: u32,
    /// Departure callback ID, or zero for no callback.
    pub departure_event: u32,
}

/// Event clock built by native `007F8660`; departure includes a stop's delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportRouteEvent {
    /// Absolute offset from the beginning of the whole route.
    pub time_ms: u32,
    /// Nonzero event callback ID from TaxiPathNode.
    pub event_id: u32,
}

/// Route placement including admitted TransportPhysics bobbing and banking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportRouteSample {
    /// Map containing the sampled section.
    pub map_id: u32,
    /// Index of the continuous section, including same-map teleport boundaries.
    pub section_index: usize,
    /// Absolute world position in yards, including vertical bobbing.
    pub position: Vec3,
    /// Native backwards tangent yaw used by MO transport models.
    pub yaw: f32,
    /// Stand, acceleration, cruise, or deceleration model sequence ID.
    pub animation_id: u32,
    /// Instantaneous route speed in yards per second, before wave attenuation.
    pub speed: f32,
    /// Native X-axis banking/wave angle, in radians.
    pub roll: f32,
    /// Native Y-axis wave angle, in radians.
    pub pitch: f32,
}

/// Invalid route admission cannot create replacement geometry or timing.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum TransportRouteError {
    /// Template motion properties cannot describe a finite forward traversal.
    #[error("transport route has invalid speed or acceleration")]
    InvalidMotion,
    /// A continuous section has missing or nonfinite spline controls.
    #[error("transport route has invalid section geometry: {0}")]
    Geometry(#[from] MovementPathError),
    /// Native finalization does not initialize the skipped terminal-control stop.
    #[error("transport route has a stop on its final control point")]
    FinalControlStop,
    /// The section cannot supply a finite nonzero distance divisor.
    #[error("transport route has no traversable distance")]
    NoDistance,
    /// A computed leg duration cannot satisfy the native signed FISTP contract.
    #[error("transport route duration is not representable in native milliseconds")]
    UnrepresentableDuration,
}

/// Current event leg's time origin and arrival profile during construction.
struct EventLeg {
    previous_distance: f32,
    departure_ms: u32,
    profile: LegProfile,
    stop: Option<(usize, u32)>,
}

/// An absolute route stop, including each section's terminal zero-delay stop.
#[derive(Clone, Copy, Debug)]
struct RouteStop {
    arrival_ms: u32,
    distance: f32,
    delay_ms: u32,
}

/// A continuous spline on one map, delimited by DBC map changes or flag one.
#[derive(Clone, Debug)]
struct RouteSection {
    map_id: u32,
    path: MovementPath,
    start_ms: u32,
    end_ms: u32,
    stops: Vec<RouteStop>,
}

/// Native MO route, retaining geometry and stop clocks for allocation-free sampling.
#[derive(Clone, Debug)]
pub struct TransportRoute {
    speed: f64,
    acceleration: f64,
    period_ms: u32,
    sections: Vec<RouteSection>,
    events: Vec<TransportRouteEvent>,
    physics: Option<TransportRoutePhysics>,
}

impl TransportRoute {
    /// Builds `007F92F0` from one path's DBC rows in their stored order.
    /// An absent path is a valid empty owner, matching the failed native lookup.
    ///
    /// # Errors
    /// Rejects nonfinite/nonpositive motion properties and malformed spline
    /// sections at the safe admission boundary; native assumes valid DBC data.
    pub fn new(
        nodes: &[TransportRouteNode],
        speed: f32,
        acceleration: f32,
    ) -> Result<Self, TransportRouteError> {
        if !speed.is_finite() || speed <= 0.0 || !acceleration.is_finite() || acceleration <= 0.0 {
            return Err(TransportRouteError::InvalidMotion);
        }
        let mut route = Self {
            speed: f64::from(speed),
            acceleration: f64::from(acceleration),
            period_ms: 0,
            sections: Vec::new(),
            events: Vec::new(),
            physics: None,
        };
        let mut start = 0;
        for end in 1..=nodes.len() {
            if end == nodes.len()
                || nodes[end].map_id != nodes[start].map_id
                || nodes[end - 1].flags & 1 != 0
            {
                route.add_section(&nodes[start..end])?;
                start = end;
            }
        }
        Ok(route)
    }

    /// Wrap duration, including station delays and any server period override.
    pub const fn period_ms(&self) -> u32 {
        self.period_ms
    }

    /// Ordered native event callbacks, retained across server period changes.
    pub fn events(&self) -> &[TransportRouteEvent] {
        &self.events
    }

    /// A missing DBC row disables bobbing and banking, as native's null pointer does.
    pub fn set_physics(&mut self, physics: Option<TransportRoutePhysics>) {
        self.physics = physics;
    }

    /// `007F7FC0` replaces the wrap period and last section end only.
    /// The server's GAMEOBJECT_LEVEL value does not rescale existing stop times.
    pub fn set_period_ms(&mut self, period_ms: u32) {
        self.period_ms = period_ms;
        if let Some(section) = self.sections.last_mut() {
            section.end_ms = period_ms;
        }
    }

    /// `007F7D30` searches real stops only, then falls back to the first
    /// section's first record (its terminal stop when that section has none).
    pub(super) fn next_departure_ms(&self, time_ms: u32) -> Option<u32> {
        for section in &self.sections {
            if time_ms >= section.end_ms {
                continue;
            }
            for stop in section.stops.iter().take(section.stops.len() - 1) {
                let departure = stop.arrival_ms.wrapping_add(stop.delay_ms);
                if time_ms < departure {
                    return Some(departure);
                }
            }
        }
        self.sections
            .first()?
            .stops
            .first()
            .map(|stop| stop.arrival_ms.wrapping_add(stop.delay_ms))
    }

    /// Samples `007F82B0` at an absolute transport clock before stop overrides.
    /// Empty owners or clocks outside all sections produce no map placement.
    #[must_use]
    pub fn sample(&self, time_ms: u32) -> Option<TransportRouteSample> {
        let clock = if self.period_ms == 0 {
            0
        } else {
            time_ms % self.period_ms
        };
        let (section_index, section) = self
            .sections
            .iter()
            .enumerate()
            .find(|(_, section)| clock < section.end_ms)?;
        let mut start_ms = section.start_ms;
        let mut start_distance = 0.0;
        let mut distance = 0.0;
        let mut animation_id = 0;
        let mut speed = 0.0;
        for (index, stop) in section.stops.iter().enumerate() {
            let last = index + 1 == section.stops.len();
            if !last && clock >= stop.arrival_ms {
                if clock.wrapping_sub(stop.arrival_ms) < stop.delay_ms {
                    distance = f64::from(stop.distance);
                    break;
                }
                start_distance = stop.distance;
                start_ms = stop.arrival_ms.wrapping_add(stop.delay_ms);
                continue;
            }
            // The literal is stored as f32 in the executable, then used by x87.
            let seconds = f64::from(0.001_f32);
            let elapsed = f64::from(clock.wrapping_sub(start_ms)) * seconds;
            let duration = f64::from(stop.arrival_ms.wrapping_sub(start_ms)) * seconds;
            let profile = match (index == 0, last) {
                (true, true) => LegProfile::Constant,
                (true, false) => LegProfile::Decelerating,
                (false, true) => LegProfile::Accelerating,
                (false, false) => LegProfile::BetweenStops,
            };
            let sample = sample_leg(elapsed, duration, self.speed, self.acceleration, profile);
            distance = sample.0 + f64::from(start_distance);
            animation_id = sample.1;
            speed = sample.2;
            break;
        }
        let fraction = (distance / f64::from(section.path.length())).clamp(0.0, 1.0) as f32;
        let sample = section.path.sample(fraction, Vec3::X);
        let yaw = (-f64::from(sample.direction.y)).atan2(-f64::from(sample.direction.x)) as f32;
        let mut position = sample.position;
        let (roll, pitch) = self.physics.map_or((0.0, 0.0), |physics| {
            physics.apply(
                &section.path,
                clock,
                speed,
                distance as f32,
                yaw,
                &mut position,
            )
        });
        Some(TransportRouteSample {
            map_id: section.map_id,
            section_index,
            position,
            yaw,
            animation_id,
            speed,
            roll,
            pitch,
        })
    }

    /// Finalizes native `007F8660`, retaining float stop distances separately
    /// from the extended-precision length sums used to compute arrivals.
    fn add_section(&mut self, nodes: &[TransportRouteNode]) -> Result<(), TransportRouteError> {
        let path = MovementPath::new(
            nodes.iter().map(|node| node.position).collect(),
            MovementPathMode::Smooth,
        )?;
        if !path.length().is_finite() || path.length() <= 0.0 {
            return Err(TransportRouteError::NoDistance);
        }
        if nodes[nodes.len() - 1].flags & 2 != 0 {
            return Err(TransportRouteError::FinalControlStop);
        }
        let mut stops = Vec::new();
        let mut departure_ms = self.period_ms;
        let mut previous_distance = 0.0_f32;
        let mut event_index = 0;
        for (index, node) in nodes
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, node)| node.flags & 2 != 0)
        {
            let profile = if stops.is_empty() {
                LegProfile::Decelerating
            } else {
                LegProfile::BetweenStops
            };
            self.add_events(
                &nodes[event_index..=index],
                event_index,
                &path,
                EventLeg {
                    previous_distance,
                    departure_ms,
                    profile,
                    stop: Some((index, node.delay_seconds.wrapping_mul(1000))),
                },
            )?;
            event_index = index + 1;
            let distance = path.distance_to_control(index);
            // 007F7A60 receives a float distance; the first leg stays in x87.
            let delta = distance - f64::from(previous_distance);
            let delta = if stops.is_empty() {
                delta
            } else {
                f64::from(delta as f32)
            };
            let arrival_ms = departure_ms.wrapping_add(duration_ms(
                delta,
                self.speed,
                self.acceleration,
                profile,
            )?);
            let delay_ms = node.delay_seconds.wrapping_mul(1000);
            stops.push(RouteStop {
                arrival_ms,
                distance: distance as f32,
                delay_ms,
            });
            previous_distance = distance as f32;
            departure_ms = arrival_ms.wrapping_add(delay_ms);
        }
        let profile = if stops.is_empty() {
            LegProfile::Constant
        } else {
            LegProfile::Accelerating
        };
        self.add_events(
            &nodes[event_index..],
            event_index,
            &path,
            EventLeg {
                previous_distance,
                departure_ms,
                profile,
                stop: None,
            },
        )?;
        let end_ms = departure_ms.wrapping_add(duration_ms(
            f64::from(path.length()) - f64::from(previous_distance),
            self.speed,
            self.acceleration,
            profile,
        )?);
        stops.push(RouteStop {
            arrival_ms: end_ms,
            distance: path.length(),
            delay_ms: 0,
        });
        self.sections.push(RouteSection {
            map_id: nodes[0].map_id,
            path,
            start_ms: self.period_ms,
            end_ms,
            stops,
        });
        self.period_ms = end_ms;
        Ok(())
    }

    /// Event timing uses the current leg's profile independently of its stop
    /// duration, including separate arrival/departure callbacks at a stop.
    fn add_events(
        &mut self,
        nodes: &[TransportRouteNode],
        first_index: usize,
        path: &MovementPath,
        leg: EventLeg,
    ) -> Result<(), TransportRouteError> {
        for (offset, node) in nodes.iter().enumerate() {
            if node.arrival_event == 0 && node.departure_event == 0 {
                continue;
            }
            let index = first_index + offset;
            let distance = path.distance_to_control(index) - f64::from(leg.previous_distance);
            let time_ms = leg.departure_ms.wrapping_add(duration_ms(
                distance,
                self.speed,
                self.acceleration,
                leg.profile,
            )?);
            if node.arrival_event != 0 {
                self.events.push(TransportRouteEvent {
                    time_ms,
                    event_id: node.arrival_event,
                });
            }
            if node.departure_event != 0 {
                let delay = leg
                    .stop
                    .filter(|(stop_index, _)| *stop_index == index)
                    .map_or(0, |(_, delay)| delay);
                self.events.push(TransportRouteEvent {
                    time_ms: time_ms.wrapping_add(delay),
                    event_id: node.departure_event,
                });
            }
        }
        Ok(())
    }
}
