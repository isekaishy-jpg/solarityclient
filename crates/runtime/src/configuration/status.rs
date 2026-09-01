//! Explicit startup-configuration failures.

use thiserror::Error;

use std::path::PathBuf;

use solarity_asset::AssetError;
use solarity_network::TransportError;

/// A failure to construct complete typed runtime configuration.
#[derive(Debug, Error)]
pub enum ConfigurationError {
    /// The explicit persistent profile root is unavailable or not a directory.
    #[error("invalid --profile-root value {path}: {message}")]
    InvalidProfileRoot {
        /// Caller-supplied profile directory.
        path: PathBuf,
        /// Filesystem context.
        message: String,
    },
    /// The stock Config.wtf profile could not be read.
    #[error("failed to read client profile {path}: {message}")]
    ProfileRead {
        /// Concrete profile file.
        path: PathBuf,
        /// Filesystem context.
        message: String,
    },
    /// One startup CVar declaration was malformed.
    #[error("invalid client profile {path} line {line}: {message}")]
    ProfileParse {
        /// Concrete profile file.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// Stable parse context.
        message: String,
    },
    /// A consumed startup CVar could not be persisted.
    #[error("failed to write client profile {path}: {message}")]
    ProfileWrite {
        /// Concrete profile file or directory.
        path: PathBuf,
        /// Filesystem context.
        message: String,
    },
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
    /// A zero-based index is malformed or negative.
    #[error("value {value} for startup option {option} must be a nonnegative integer")]
    InvalidNonnegativeInteger {
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
    /// The login-server authority is not valid `host:port` syntax.
    #[error("invalid --login-endpoint value: {source}")]
    InvalidLoginEndpoint {
        /// The transport boundary's contextual validation failure.
        #[source]
        source: TransportError,
    },
    /// The login timezone cannot be represented by the signed wire field.
    #[error("value {value} for --login-timezone-minutes must be a signed 32-bit integer")]
    InvalidLoginTimezone {
        /// The rejected value.
        value: String,
    },
    /// The login challenge requires an explicit IPv4 address.
    #[error("value {value} for --login-client-ip must be an IPv4 address")]
    InvalidLoginClientIp {
        /// The rejected value.
        value: String,
    },
}
