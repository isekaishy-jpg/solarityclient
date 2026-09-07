//! Listener-space and FMOD 3D policy for one advanced sound voice.

use glam::Vec3;
use solarity_asset::SoundEntry;
use solarity_rendering::WorldCameraFrame;
use thiserror::Error;

use crate::audio::backend::SoundSpatialPosition;

use super::sound_interface2_advanced_kit_properties::AdvancedSoundProperties;

const BASIS_TOLERANCE: f32 = 1.0e-4;

/// Validated listener origin and orthonormal world-space basis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedSoundListener {
    position: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
}

impl AdvancedSoundListener {
    /// Validates an explicit listener frame without repairing its basis.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundSpatialError`] for non-finite, non-unit, or
    /// non-orthogonal input instead of constructing a fallback orientation.
    pub fn new(
        position: Vec3,
        forward: Vec3,
        right: Vec3,
        up: Vec3,
    ) -> Result<Self, AdvancedSoundSpatialError> {
        if !position.is_finite() || !forward.is_finite() || !right.is_finite() || !up.is_finite() {
            return Err(AdvancedSoundSpatialError::NonFiniteListener);
        }
        if !is_unit(forward)
            || !is_unit(right)
            || !is_unit(up)
            || forward.dot(right).abs() > BASIS_TOLERANCE
            || forward.dot(up).abs() > BASIS_TOLERANCE
            || right.dot(up).abs() > BASIS_TOLERANCE
            || forward.cross(up).dot(right) < 1.0 - BASIS_TOLERANCE
        {
            return Err(AdvancedSoundSpatialError::ListenerBasis);
        }
        Ok(Self {
            position,
            forward,
            right,
            up,
        })
    }

    /// Copies the exact orthonormal basis already validated for rendering.
    #[must_use]
    pub fn from_world_camera(frame: WorldCameraFrame) -> Self {
        Self {
            position: frame.camera().position(),
            forward: frame.forward(),
            right: frame.right(),
            up: frame.up(),
        }
    }

    /// Uses stock's character-relative listener origin with the camera basis.
    /// `0x004fa5f0` subtracts the camera forward times the back distance, then
    /// adds the up distance on the world Z axis rather than the pitched camera up.
    ///
    /// # Errors
    /// Returns [`AdvancedSoundSpatialError::NonFiniteListener`] for non-finite
    /// coordinates, offsets, or the resulting origin.
    pub fn at_character(
        frame: WorldCameraFrame,
        position: Vec3,
        back_distance: f32,
        up_distance: f32,
    ) -> Result<Self, AdvancedSoundSpatialError> {
        let mut listener = Self::from_world_camera(frame);
        listener.position = position - frame.forward() * back_distance + Vec3::Z * up_distance;
        if !back_distance.is_finite() || !up_distance.is_finite() || !listener.position.is_finite()
        {
            return Err(AdvancedSoundSpatialError::NonFiniteListener);
        }
        Ok(listener)
    }

    /// Returns the listener's world-space origin.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }
}

/// Stock FMOD 3D values evaluated for one advanced sound and listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedSoundSpatialMix {
    backend_position: Option<SoundSpatialPosition>,
    pan_level: f32,
    distance_gain: f32,
    cone_gain: f32,
    three_dimensional_gain: f32,
}

impl AdvancedSoundSpatialMix {
    /// Evaluates an ordinary fully positional `SoundEntries` voice.
    ///
    /// Model callbacks enter the stock `playSoundEntryAt` path without an
    /// advanced row, so they have neither an advanced pan-radius blend nor an
    /// emitter cone. They still use the base row's FMOD minimum and cutoff
    /// distances and the same listener-space conversion as advanced sounds.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundSpatialError`] for an invalid emitter or
    /// authored distance interval. No distance or coordinate is repaired.
    pub fn evaluate_positioned(
        listener: AdvancedSoundListener,
        emitter_position: Vec3,
        sound_entry: &SoundEntry,
    ) -> Result<Self, AdvancedSoundSpatialError> {
        if !emitter_position.is_finite() {
            return Err(AdvancedSoundSpatialError::NonFiniteEmitter);
        }
        let minimum_distance = sound_entry.minimum_distance();
        let maximum_distance = sound_entry.distance_cutoff();
        if minimum_distance < 0.0 || maximum_distance < minimum_distance {
            return Err(AdvancedSoundSpatialError::DistanceRange {
                minimum: minimum_distance,
                maximum: maximum_distance,
            });
        }

        let listener_to_emitter = emitter_position - listener.position;
        let distance_gain = stock_distance_gain(
            listener_to_emitter.length(),
            minimum_distance,
            maximum_distance,
        );
        Ok(Self {
            backend_position: Some(listener_position(listener, listener_to_emitter)?),
            pan_level: 1.0,
            distance_gain,
            cone_gain: 1.0,
            three_dimensional_gain: distance_gain,
        })
    }

    /// Evaluates listener coordinates, stock distance roll-off, cone, and 3D blend.
    ///
    /// Build 12340 passes `SoundEntries` minimum/maximum distances and the
    /// corrected advanced cone values to FMOD. Its installed callback at
    /// 0x00878320 overrides FMOD's default attenuation, reaching silence at
    /// the authored cutoff. The 3D pan level blends with the unattenuated 2D path.
    ///
    /// The SDL position is normalized to unit distance so SDL contributes only
    /// speaker direction; the returned gain retains the stock FMOD distance
    /// and cone behavior without applying SDL's fixed one-unit curve twice.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundSpatialError`] for invalid runtime vectors or
    /// authored FMOD parameters. No value is clamped into a valid range.
    pub fn evaluate(
        listener: AdvancedSoundListener,
        emitter_position: Vec3,
        cone_orientation: Vec3,
        properties: AdvancedSoundProperties,
        sound_entry: &SoundEntry,
    ) -> Result<Self, AdvancedSoundSpatialError> {
        if !emitter_position.is_finite() || !cone_orientation.is_finite() {
            return Err(AdvancedSoundSpatialError::NonFiniteEmitter);
        }
        let minimum_distance = sound_entry.minimum_distance();
        let maximum_distance = sound_entry.distance_cutoff();
        if minimum_distance < 0.0 || maximum_distance < minimum_distance {
            return Err(AdvancedSoundSpatialError::DistanceRange {
                minimum: minimum_distance,
                maximum: maximum_distance,
            });
        }
        let [inside_angle, outside_angle] = properties.cone_angles();
        let outside_gain = properties.outside_cone_gain();
        if !(0.0..=360.0).contains(&inside_angle)
            || !(inside_angle..=360.0).contains(&outside_angle)
            || !(0.0..=1.0).contains(&outside_gain)
        {
            return Err(AdvancedSoundSpatialError::Cone {
                inside_angle,
                outside_angle,
                outside_gain,
            });
        }

        // A zero authored emitter position is passed to the stock play path as
        // a null vector and remains a non-positional sound.
        if emitter_position == Vec3::ZERO {
            return Ok(Self {
                backend_position: None,
                pan_level: 0.0,
                distance_gain: 1.0,
                cone_gain: 1.0,
                three_dimensional_gain: 1.0,
            });
        }

        let listener_to_emitter = emitter_position - listener.position;
        let distance = listener_to_emitter.length();
        let pan_level =
            properties.pan_level(listener.position.to_array(), emitter_position.to_array());
        let distance_gain = stock_distance_gain(distance, minimum_distance, maximum_distance);
        let cone_gain = cone_attenuation(
            listener.position - emitter_position,
            cone_orientation,
            inside_angle,
            outside_angle,
            outside_gain,
        );
        let three_dimensional_gain = 1.0 + (distance_gain * cone_gain - 1.0) * pan_level;
        let backend_position = listener_position(listener, listener_to_emitter)?;

        Ok(Self {
            backend_position: Some(backend_position),
            pan_level,
            distance_gain,
            cone_gain,
            three_dimensional_gain,
        })
    }

    /// Returns the unit-distance SDL listener coordinate, or no 3D position.
    #[must_use]
    pub const fn backend_position(self) -> Option<SoundSpatialPosition> {
        self.backend_position
    }

    /// Returns stock's current 2D-to-3D blend.
    #[must_use]
    pub const fn pan_level(self) -> f32 {
        self.pan_level
    }

    /// Returns the stock rolloff callback's gain before the pan-level blend.
    #[must_use]
    pub const fn distance_gain(self) -> f32 {
        self.distance_gain
    }

    /// Returns directional-cone attenuation before the pan-level blend.
    #[must_use]
    pub const fn cone_gain(self) -> f32 {
        self.cone_gain
    }

    /// Returns the FMOD 3D attenuation after blending with the 2D path.
    #[must_use]
    pub const fn three_dimensional_gain(self) -> f32 {
        self.three_dimensional_gain
    }
}

/// Exact data failure while evaluating an advanced spatial voice.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum AdvancedSoundSpatialError {
    /// A listener position or basis vector was non-finite.
    #[error("advanced sound listener contains a non-finite value")]
    NonFiniteListener,
    /// Listener axes were not a right-handed orthonormal basis.
    #[error("advanced sound listener basis is not right-handed and orthonormal")]
    ListenerBasis,
    /// An emitter position or cone direction was non-finite.
    #[error("advanced sound emitter contains a non-finite value")]
    NonFiniteEmitter,
    /// `SoundEntries.dbc` contains an invalid FMOD distance interval.
    #[error("advanced sound distance range is invalid: {minimum}..{maximum}")]
    DistanceRange {
        /// Unmodified minimum distance.
        minimum: f32,
        /// Unmodified maximum distance.
        maximum: f32,
    },
    /// Corrected advanced cone parameters remain outside FMOD's domain.
    #[error(
        "advanced sound cone is invalid: inside {inside_angle}, outside {outside_angle}, gain {outside_gain}"
    )]
    Cone {
        /// Corrected inner spread in degrees.
        inside_angle: f32,
        /// Corrected outer spread in degrees.
        outside_angle: f32,
        /// Authored gain outside the cone.
        outside_gain: f32,
    },
    /// A finite world displacement could not form an SDL coordinate.
    #[error("advanced sound listener-relative position is invalid")]
    BackendPosition,
}

/// Tests a normalized camera axis within the renderer's construction error.
fn is_unit(axis: Vec3) -> bool {
    (axis.length_squared() - 1.0).abs() <= BASIS_TOLERANCE
}

/// Reproduces 0x008782B0: fourfold rolloff and a taper over the last 10%.
/// The cutoff comparison precedes the minimum comparison, including equal
/// endpoints. The x87 path retains intermediate precision until FMOD stores f32.
fn stock_distance_gain(distance: f32, minimum: f32, maximum: f32) -> f32 {
    if distance >= maximum {
        0.0
    } else if distance <= minimum {
        1.0
    } else {
        let distance = f64::from(distance);
        let minimum = f64::from(minimum);
        let maximum = f64::from(maximum);
        let gain = minimum / ((distance - minimum) * 4.0 + minimum);
        // The executable constant at A02A34 is f32 0.9, extended for x87.
        let taper_start = maximum * f64::from(0.9_f32);
        if distance > taper_start {
            (gain * (1.0 - (distance - taper_start) / (maximum - taper_start))) as f32
        } else {
            gain as f32
        }
    }
}

/// Applies FMOD's linear transition across full cone-spread angles.
fn cone_attenuation(
    emitter_to_listener: Vec3,
    orientation: Vec3,
    inside_angle: f32,
    outside_angle: f32,
    outside_gain: f32,
) -> f32 {
    if orientation == Vec3::ZERO || emitter_to_listener == Vec3::ZERO {
        return 1.0;
    }
    let spread_angle = orientation
        .normalize()
        .dot(emitter_to_listener.normalize())
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
        * 2.0;
    if spread_angle <= inside_angle {
        1.0
    } else if outside_angle <= spread_angle {
        outside_gain
    } else {
        let amount = (spread_angle - inside_angle) / (outside_angle - inside_angle);
        1.0 + (outside_gain - 1.0) * amount
    }
}

/// Converts a world displacement to SDL's right, up, back unit frame.
fn listener_position(
    listener: AdvancedSoundListener,
    displacement: Vec3,
) -> Result<SoundSpatialPosition, AdvancedSoundSpatialError> {
    let direction = if displacement == Vec3::ZERO {
        Vec3::ZERO
    } else {
        displacement.normalize()
    };
    SoundSpatialPosition::new([
        direction.dot(listener.right),
        direction.dot(listener.up),
        -direction.dot(listener.forward),
    ])
    .map_err(|_source| AdvancedSoundSpatialError::BackendPosition)
}
