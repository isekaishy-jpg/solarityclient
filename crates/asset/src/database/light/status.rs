//! Stable failures for strict world-light selection and sampling.

use thiserror::Error;

/// Authored environment state cannot produce a complete stock light sample.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldLightSampleError {
    /// The query position contains NaN or infinity.
    #[error("world light query position is not finite")]
    NonFinitePosition,
    /// Neither the active map's global light nor fallback Light.dbc ID 1 exists.
    #[error("map {map_id} has no global or fallback light for condition {condition}")]
    MissingGlobalLight {
        /// Active Map.dbc identifier.
        map_id: u32,
        /// Zero-based light-condition slot.
        condition: u8,
    },
    /// An explicitly selected Light.dbc identifier is absent.
    #[error("world light override {light_id} does not exist")]
    MissingLightOverride {
        /// Requested Light.dbc identifier.
        light_id: u32,
    },
    /// A light's selected parameter slot is zero or absent.
    #[error("light {light_id} has no parameters for condition {condition}")]
    MissingParameter {
        /// Light.dbc identifier.
        light_id: u32,
        /// Zero-based light-condition slot.
        condition: u8,
    },
    /// A direct LightParams lookup has no matching row.
    #[error("light parameters {parameter_id} do not exist")]
    MissingParameterId {
        /// Requested LightParams.dbc identifier.
        parameter_id: u32,
    },
    /// One of the required 18 color channels is absent.
    #[error("world light color band {band_id} is missing")]
    MissingColorBand {
        /// LightIntBand.dbc identifier.
        band_id: u32,
    },
    /// One of the required six scalar channels is absent.
    #[error("world light float band {band_id} is missing")]
    MissingFloatBand {
        /// LightFloatBand.dbc identifier.
        band_id: u32,
    },
}
