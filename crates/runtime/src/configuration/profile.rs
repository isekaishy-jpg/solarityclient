//! Validated process-startup configuration.

use std::ffi::OsString;
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use solarity_asset::{ClientDataRoot, Locale};
use solarity_cpu::CpuPoolConfig;
use solarity_network::{GruntLoginOptions, TcpEndpoint};

use crate::configuration::login::{client_ip, login_locale};
use crate::configuration::{
    ConfigurationError, LoginConfiguration, WindowConfiguration, WindowMode,
};

const DATA_ROOT_OPTION: &str = "--data-root";
const PROFILE_ROOT_OPTION: &str = "--profile-root";
const LOCALE_OPTION: &str = "--locale";
const CPU_WORKERS_OPTION: &str = "--cpu-workers";
const CPU_CAPACITY_OPTION: &str = "--cpu-capacity";
const NETWORK_WORKERS_OPTION: &str = "--network-workers";
const NETWORK_SHUTDOWN_OPTION: &str = "--network-shutdown-ms";
const LOGIN_ENDPOINT_OPTION: &str = "--login-endpoint";
const LOGIN_TIMEZONE_OPTION: &str = "--login-timezone-minutes";
const LOGIN_CLIENT_IP_OPTION: &str = "--login-client-ip";
const WINDOW_WIDTH_OPTION: &str = "--window-width";
const WINDOW_HEIGHT_OPTION: &str = "--window-height";
const WINDOW_MODE_OPTION: &str = "--window-mode";
const GPU_INDEX_OPTION: &str = "--gpu-index";

/// Complete configuration required to construct the initial client services.
#[derive(Clone, Debug)]
pub struct RuntimeConfiguration {
    data_root: ClientDataRoot,
    profile_root: PathBuf,
    locale: Locale,
    cpu_pool: CpuPoolConfig,
    network_workers: NonZeroUsize,
    network_shutdown_timeout: Duration,
    login: LoginConfiguration,
    window: WindowConfiguration,
    gpu_index: usize,
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
                PROFILE_ROOT_OPTION => {
                    let value = next_value(&mut arguments, PROFILE_ROOT_OPTION)?;
                    set_once(&mut values.profile_root, value, PROFILE_ROOT_OPTION)?;
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
                LOGIN_ENDPOINT_OPTION => {
                    let value = next_value(&mut arguments, LOGIN_ENDPOINT_OPTION)?;
                    set_once(&mut values.login_endpoint, value, LOGIN_ENDPOINT_OPTION)?;
                }
                LOGIN_TIMEZONE_OPTION => {
                    let value = next_value(&mut arguments, LOGIN_TIMEZONE_OPTION)?;
                    set_once(&mut values.login_timezone, value, LOGIN_TIMEZONE_OPTION)?;
                }
                LOGIN_CLIENT_IP_OPTION => {
                    let value = next_value(&mut arguments, LOGIN_CLIENT_IP_OPTION)?;
                    set_once(&mut values.login_client_ip, value, LOGIN_CLIENT_IP_OPTION)?;
                }
                WINDOW_WIDTH_OPTION => {
                    let value = next_value(&mut arguments, WINDOW_WIDTH_OPTION)?;
                    set_once(&mut values.window_width, value, WINDOW_WIDTH_OPTION)?;
                }
                WINDOW_HEIGHT_OPTION => {
                    let value = next_value(&mut arguments, WINDOW_HEIGHT_OPTION)?;
                    set_once(&mut values.window_height, value, WINDOW_HEIGHT_OPTION)?;
                }
                WINDOW_MODE_OPTION => {
                    let value = next_value(&mut arguments, WINDOW_MODE_OPTION)?;
                    set_once(&mut values.window_mode, value, WINDOW_MODE_OPTION)?;
                }
                GPU_INDEX_OPTION => {
                    let value = next_value(&mut arguments, GPU_INDEX_OPTION)?;
                    set_once(&mut values.gpu_index, value, GPU_INDEX_OPTION)?;
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
        "solarity-runtime --data-root <Data> --profile-root <directory> --locale <locale> \
         --cpu-workers <count> \
         --cpu-capacity <count> --network-workers <count> --network-shutdown-ms <milliseconds> \
         --login-endpoint <host:port> --login-timezone-minutes <signed-minutes> \
         --login-client-ip <IPv4> \
         --window-width <pixels> --window-height <pixels> \
         --window-mode <windowed|fullscreen-windowed> \
         --gpu-index <zero-based-index>"
    }

    /// Returns the validated client `Data` directory.
    #[must_use]
    pub fn data_root(&self) -> &ClientDataRoot {
        &self.data_root
    }

    /// Returns the explicit root containing stock `WTF` profile state.
    #[must_use]
    pub fn profile_root(&self) -> &std::path::Path {
        &self.profile_root
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

    /// Returns the exact login-server and challenge identity.
    #[must_use]
    pub const fn login(&self) -> &LoginConfiguration {
        &self.login
    }

    /// Returns the validated initial SDL window policy.
    #[must_use]
    pub const fn window(&self) -> WindowConfiguration {
        self.window
    }

    /// Returns the explicit zero-based Vulkan physical-device index.
    #[must_use]
    pub const fn gpu_index(&self) -> usize {
        self.gpu_index
    }

    /// Validates parsed operating-system strings into domain types.
    fn from_parsed(values: ParsedValues) -> Result<Self, ConfigurationError> {
        let data_root = required(values.data_root, DATA_ROOT_OPTION)?;
        let profile_root = required(values.profile_root, PROFILE_ROOT_OPTION)?;
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
        let login_endpoint = required(values.login_endpoint, LOGIN_ENDPOINT_OPTION)?;
        let login_endpoint_text = unicode(&login_endpoint, LOGIN_ENDPOINT_OPTION)?;
        let login_endpoint = TcpEndpoint::parse(login_endpoint_text)
            .map_err(|source| ConfigurationError::InvalidLoginEndpoint { source })?;
        let login_timezone = required(values.login_timezone, LOGIN_TIMEZONE_OPTION)?;
        let login_timezone_text = unicode(&login_timezone, LOGIN_TIMEZONE_OPTION)?;
        let login_timezone = login_timezone_text.parse::<i32>().map_err(|_source| {
            ConfigurationError::InvalidLoginTimezone {
                value: login_timezone_text.to_owned(),
            }
        })?;
        let login_client_ip = required(values.login_client_ip, LOGIN_CLIENT_IP_OPTION)?;
        let login_client_ip_text = unicode(&login_client_ip, LOGIN_CLIENT_IP_OPTION)?;
        let login_client_ip = client_ip(login_client_ip_text).map_err(|_source| {
            ConfigurationError::InvalidLoginClientIp {
                value: login_client_ip_text.to_owned(),
            }
        })?;
        let window_width = positive_u32(
            required(values.window_width, WINDOW_WIDTH_OPTION)?,
            WINDOW_WIDTH_OPTION,
        )?;
        let window_height = positive_u32(
            required(values.window_height, WINDOW_HEIGHT_OPTION)?,
            WINDOW_HEIGHT_OPTION,
        )?;
        let window_mode = required(values.window_mode, WINDOW_MODE_OPTION)?;
        let window_mode = WindowMode::parse(unicode(&window_mode, WINDOW_MODE_OPTION)?)
            .ok_or_else(|| ConfigurationError::InvalidWindowMode {
                value: window_mode.to_string_lossy().into_owned(),
            })?;
        let gpu_index = nonnegative_integer(
            required(values.gpu_index, GPU_INDEX_OPTION)?,
            GPU_INDEX_OPTION,
        )?;

        let data_root = ClientDataRoot::new(PathBuf::from(data_root))
            .map_err(|source| ConfigurationError::InvalidDataRoot { source })?;
        let profile_root = PathBuf::from(profile_root);
        let profile_root = fs::canonicalize(&profile_root).map_err(|source| {
            ConfigurationError::InvalidProfileRoot {
                path: profile_root.clone(),
                message: source.to_string(),
            }
        })?;
        if !profile_root.is_dir() {
            return Err(ConfigurationError::InvalidProfileRoot {
                path: profile_root,
                message: "path is not a directory".to_owned(),
            });
        }
        let locale = Locale::from_str(locale)
            .map_err(|source| ConfigurationError::InvalidLocale { source })?;
        let login = LoginConfiguration::new(
            login_endpoint,
            GruntLoginOptions::new(login_locale(locale), login_timezone, login_client_ip),
        );
        let shutdown_milliseconds =
            u64::try_from(network_shutdown_ms.get()).map_err(|_source| {
                ConfigurationError::InvalidPositiveInteger {
                    option: NETWORK_SHUTDOWN_OPTION,
                    value: network_shutdown_ms.to_string(),
                }
            })?;

        Ok(Self {
            data_root,
            profile_root,
            locale,
            cpu_pool: CpuPoolConfig::new(cpu_workers, cpu_capacity),
            network_workers,
            network_shutdown_timeout: Duration::from_millis(shutdown_milliseconds),
            login,
            window: WindowConfiguration::new(window_width, window_height, window_mode),
            gpu_index,
        })
    }
}

/// Raw values collected before required-option and type validation.
#[derive(Default)]
struct ParsedValues {
    data_root: Option<OsString>,
    profile_root: Option<OsString>,
    locale: Option<OsString>,
    cpu_workers: Option<OsString>,
    cpu_capacity: Option<OsString>,
    network_workers: Option<OsString>,
    network_shutdown_ms: Option<OsString>,
    login_endpoint: Option<OsString>,
    login_timezone: Option<OsString>,
    login_client_ip: Option<OsString>,
    window_width: Option<OsString>,
    window_height: Option<OsString>,
    window_mode: Option<OsString>,
    gpu_index: Option<OsString>,
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

/// Parses a nonzero SDL dimension that also fits SDL's signed C interface.
fn positive_u32(value: OsString, option: &'static str) -> Result<u32, ConfigurationError> {
    let text = unicode(&value, option)?;
    let dimension =
        text.parse::<u32>()
            .map_err(|_source| ConfigurationError::InvalidWindowDimension {
                option,
                value: text.to_owned(),
            })?;
    if dimension == 0 || dimension > i32::MAX as u32 {
        return Err(ConfigurationError::InvalidWindowDimension {
            option,
            value: text.to_owned(),
        });
    }
    Ok(dimension)
}

/// Parses an index where zero identifies Vulkan's first enumerated adapter.
fn nonnegative_integer(value: OsString, option: &'static str) -> Result<usize, ConfigurationError> {
    let text = unicode(&value, option)?;
    text.parse::<usize>()
        .map_err(|_source| ConfigurationError::InvalidNonnegativeInteger {
            option,
            value: text.to_owned(),
        })
}
