//! Validated process-startup configuration.

use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use solarity_asset::{ClientDataRoot, Locale};
use solarity_cpu::CpuPoolConfig;

use crate::configuration::ConfigurationError;

const DATA_ROOT_OPTION: &str = "--data-root";
const LOCALE_OPTION: &str = "--locale";
const CPU_WORKERS_OPTION: &str = "--cpu-workers";
const CPU_CAPACITY_OPTION: &str = "--cpu-capacity";
const NETWORK_WORKERS_OPTION: &str = "--network-workers";
const NETWORK_SHUTDOWN_OPTION: &str = "--network-shutdown-ms";

/// Complete configuration required to construct the initial client services.
#[derive(Clone, Debug)]
pub struct RuntimeConfiguration {
    data_root: ClientDataRoot,
    locale: Locale,
    cpu_pool: CpuPoolConfig,
    network_workers: NonZeroUsize,
    network_shutdown_timeout: Duration,
}

impl RuntimeConfiguration {
    /// Parses required options without environment or guessed-default fallbacks.
    ///
    /// The iterator must exclude the executable name.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] for unknown, duplicate, missing, or
    /// invalid options and for a data directory that cannot be validated.
    pub fn from_arguments(
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<Self, ConfigurationError> {
        let mut values = ParsedValues::default();
        let mut arguments = arguments.into_iter();
        while let Some(option) = arguments.next() {
            let option = option
                .to_str()
                .ok_or(ConfigurationError::NonUnicodeOption)?;
            match option {
                DATA_ROOT_OPTION => {
                    let value = next_value(&mut arguments, DATA_ROOT_OPTION)?;
                    set_once(&mut values.data_root, value, DATA_ROOT_OPTION)?;
                }
                LOCALE_OPTION => {
                    let value = next_value(&mut arguments, LOCALE_OPTION)?;
                    set_once(&mut values.locale, value, LOCALE_OPTION)?;
                }
                CPU_WORKERS_OPTION => {
                    let value = next_value(&mut arguments, CPU_WORKERS_OPTION)?;
                    set_once(&mut values.cpu_workers, value, CPU_WORKERS_OPTION)?;
                }
                CPU_CAPACITY_OPTION => {
                    let value = next_value(&mut arguments, CPU_CAPACITY_OPTION)?;
                    set_once(&mut values.cpu_capacity, value, CPU_CAPACITY_OPTION)?;
                }
                NETWORK_WORKERS_OPTION => {
                    let value = next_value(&mut arguments, NETWORK_WORKERS_OPTION)?;
                    set_once(&mut values.network_workers, value, NETWORK_WORKERS_OPTION)?;
                }
                NETWORK_SHUTDOWN_OPTION => {
                    let value = next_value(&mut arguments, NETWORK_SHUTDOWN_OPTION)?;
                    set_once(
                        &mut values.network_shutdown_ms,
                        value,
                        NETWORK_SHUTDOWN_OPTION,
                    )?;
                }
                _ => {
                    return Err(ConfigurationError::UnknownOption {
                        option: option.to_owned(),
                    });
                }
            }
        }

        Self::from_parsed(values)
    }

    /// Returns the executable's current explicit command-line contract.
    #[must_use]
    pub const fn usage() -> &'static str {
        "solarity-runtime --data-root <Data> --locale <locale> --cpu-workers <count> \
         --cpu-capacity <count> --network-workers <count> --network-shutdown-ms <milliseconds>"
    }

    /// Returns the validated client `Data` directory.
    #[must_use]
    pub fn data_root(&self) -> &ClientDataRoot {
        &self.data_root
    }

    /// Returns the selected build-12340 locale.
    #[must_use]
    pub const fn locale(&self) -> Locale {
        self.locale
    }

    /// Returns explicit CPU worker and admission capacity.
    #[must_use]
    pub const fn cpu_pool(&self) -> CpuPoolConfig {
        self.cpu_pool
    }

    /// Returns the exact Tokio worker count.
    #[must_use]
    pub const fn network_workers(&self) -> NonZeroUsize {
        self.network_workers
    }

    /// Returns the maximum orderly Tokio shutdown wait.
    #[must_use]
    pub const fn network_shutdown_timeout(&self) -> Duration {
        self.network_shutdown_timeout
    }

    /// Validates parsed operating-system strings into domain types.
    fn from_parsed(values: ParsedValues) -> Result<Self, ConfigurationError> {
        let data_root = required(values.data_root, DATA_ROOT_OPTION)?;
        let locale = required(values.locale, LOCALE_OPTION)?;
        let locale = unicode(&locale, LOCALE_OPTION)?;
        let cpu_workers = positive_integer(
            required(values.cpu_workers, CPU_WORKERS_OPTION)?,
            CPU_WORKERS_OPTION,
        )?;
        let cpu_capacity = positive_integer(
            required(values.cpu_capacity, CPU_CAPACITY_OPTION)?,
            CPU_CAPACITY_OPTION,
        )?;
        let network_workers = positive_integer(
            required(values.network_workers, NETWORK_WORKERS_OPTION)?,
            NETWORK_WORKERS_OPTION,
        )?;
        let network_shutdown_ms = positive_integer(
            required(values.network_shutdown_ms, NETWORK_SHUTDOWN_OPTION)?,
            NETWORK_SHUTDOWN_OPTION,
        )?;

        let data_root = ClientDataRoot::new(PathBuf::from(data_root))
            .map_err(|source| ConfigurationError::InvalidDataRoot { source })?;
        let locale = Locale::from_str(locale)
            .map_err(|source| ConfigurationError::InvalidLocale { source })?;
        let shutdown_milliseconds =
            u64::try_from(network_shutdown_ms.get()).map_err(|_source| {
                ConfigurationError::InvalidPositiveInteger {
                    option: NETWORK_SHUTDOWN_OPTION,
                    value: network_shutdown_ms.to_string(),
                }
            })?;

        Ok(Self {
            data_root,
            locale,
            cpu_pool: CpuPoolConfig::new(cpu_workers, cpu_capacity),
            network_workers,
            network_shutdown_timeout: Duration::from_millis(shutdown_milliseconds),
        })
    }
}

/// Raw values collected before required-option and type validation.
#[derive(Default)]
struct ParsedValues {
    data_root: Option<OsString>,
    locale: Option<OsString>,
    cpu_workers: Option<OsString>,
    cpu_capacity: Option<OsString>,
    network_workers: Option<OsString>,
    network_shutdown_ms: Option<OsString>,
}

/// Reads the argument following an option.
fn next_value(
    arguments: &mut impl Iterator<Item = OsString>,
    option: &'static str,
) -> Result<OsString, ConfigurationError> {
    arguments
        .next()
        .ok_or(ConfigurationError::MissingValue { option })
}

/// Rejects ambiguous repeated configuration.
fn set_once(
    destination: &mut Option<OsString>,
    value: OsString,
    option: &'static str,
) -> Result<(), ConfigurationError> {
    if destination.is_some() {
        return Err(ConfigurationError::DuplicateOption { option });
    }
    *destination = Some(value);
    Ok(())
}

/// Extracts one required raw value.
fn required(value: Option<OsString>, option: &'static str) -> Result<OsString, ConfigurationError> {
    value.ok_or(ConfigurationError::MissingRequiredOption { option })
}

/// Borrows a UTF-8 option value for typed parsing.
fn unicode<'a>(value: &'a OsString, option: &'static str) -> Result<&'a str, ConfigurationError> {
    value
        .to_str()
        .ok_or(ConfigurationError::NonUnicodeValue { option })
}

/// Parses a nonzero count or duration value.
fn positive_integer(
    value: OsString,
    option: &'static str,
) -> Result<NonZeroUsize, ConfigurationError> {
    let text = unicode(&value, option)?;
    text.parse::<NonZeroUsize>()
        .map_err(|_source| ConfigurationError::InvalidPositiveInteger {
            option,
            value: text.to_owned(),
        })
}
