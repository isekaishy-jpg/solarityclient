//! Explicit build-12340 login-server identity and wire options.

use std::net::Ipv4Addr;

use solarity_asset::Locale;
use solarity_network::{GruntLoginOptions, LoginLocale, TcpEndpoint};

/// Complete transport and challenge identity for the legacy login server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginConfiguration {
    endpoint: TcpEndpoint,
    options: GruntLoginOptions,
}

impl LoginConfiguration {
    /// Creates a complete explicit login profile.
    #[must_use]
    pub const fn new(endpoint: TcpEndpoint, options: GruntLoginOptions) -> Self {
        Self { endpoint, options }
    }

    /// Returns the explicitly configured login-server authority.
    #[must_use]
    pub const fn endpoint(&self) -> &TcpEndpoint {
        &self.endpoint
    }

    /// Returns the locale, timezone, and IPv4 identity sent in the challenge.
    #[must_use]
    pub const fn options(&self) -> GruntLoginOptions {
        self.options
    }
}

/// Maps the mounted client locale to its exact login-protocol token.
pub(crate) const fn login_locale(locale: Locale) -> LoginLocale {
    match locale {
        Locale::DeDe => LoginLocale::DeDe,
        Locale::EnGb => LoginLocale::EnGb,
        Locale::EnUs => LoginLocale::EnUs,
        Locale::EsEs => LoginLocale::EsEs,
        Locale::FrFr => LoginLocale::FrFr,
        Locale::KoKr => LoginLocale::KoKr,
        Locale::ZhCn => LoginLocale::ZhCn,
        Locale::ZhTw => LoginLocale::ZhTw,
        Locale::EnCn => LoginLocale::EnCn,
        Locale::EnTw => LoginLocale::EnTw,
        Locale::EsMx => LoginLocale::EsMx,
        Locale::RuRu => LoginLocale::RuRu,
    }
}

/// Parses one explicit IPv4 identity without DNS or interface guessing.
pub(crate) fn client_ip(value: &str) -> Result<Ipv4Addr, std::net::AddrParseError> {
    value.parse()
}
