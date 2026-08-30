//! Explicit startup-configuration failures.

use thiserror::Error;

use solarity_asset::AssetError;

/// A failure to construct complete typed runtime configuration.
#[derive(Debug, Error)]
pub enum ConfigurationError {
    /// A command-line option is not part of the current startup contract.
    #[error("unknown startup option {option}")]
    UnknownOption {
        /// The rejected option.
        option: String,
    },
    /// An option token cannot be represented as UTF-8.
    #[error("startup option name is not UTF-8")]
    NonUnicodeOption,
    /// An option requiring a value was the final argument.
    #[error("startup option {option} requires a value")]
    MissingValue {
        /// The option missing its value.
        option: &'static str,
    },
    /// An option was supplied more than once.
    #[error("startup option {option} was supplied more than once")]
    DuplicateOption {
        /// The duplicate option.
        option: &'static str,
    },
    /// A required option was not supplied.
    #[error("required startup option {option} was not supplied")]
    MissingRequiredOption {
        /// The absent option.
        option: &'static str,
    },
    /// A textual option value is not UTF-8.
    #[error("value for startup option {option} is not UTF-8")]
    NonUnicodeValue {
        /// The option owning the value.
        option: &'static str,
    },
    /// A count or duration is not a valid positive integer.
    #[error("value {value} for startup option {option} must be a positive integer")]
    InvalidPositiveInteger {
        /// The option owning the value.
        option: &'static str,
        /// The rejected value.
        value: String,
    },
    /// An SDL window dimension is zero, malformed, or exceeds `i32::MAX`.
    #[error("value {value} for startup option {option} must be an SDL window dimension")]
    InvalidWindowDimension {
        /// The option owning the value.
        option: &'static str,
        /// The rejected value.
        value: String,
    },
    /// The requested initial window mode is not part of the supported contract.
    #[error("unsupported --window-mode value {value}")]
    InvalidWindowMode {
        /// The rejected mode token.
        value: String,
    },
    /// The data-root value failed filesystem validation.
    #[error("invalid --data-root value: {source}")]
    InvalidDataRoot {
        /// The asset boundary's contextual failure.
        #[source]
        source: AssetError,
    },
    /// The locale token is not supported by build 12340.
    #[error("invalid --locale value: {source}")]
    InvalidLocale {
        /// The asset boundary's contextual failure.
        #[source]
        source: AssetError,
    },
}
