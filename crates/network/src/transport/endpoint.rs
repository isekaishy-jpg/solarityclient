//! Validated login and realm TCP authorities.

use std::fmt;

use super::TransportError;

/// A validated host and nonzero TCP port.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TcpEndpoint {
    host: String,
    port: u16,
}

impl TcpEndpoint {
    /// Creates an endpoint without performing DNS resolution or connecting.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::InvalidEndpoint`] for an empty host, a host
    /// containing NUL, or port zero.
    pub fn new(host: impl Into<String>, port: u16) -> Result<Self, TransportError> {
        let host = host.into();
        if host.is_empty() {
            return Err(TransportError::InvalidEndpoint {
                authority: host,
                reason: "host cannot be empty",
            });
        }
        if host.as_bytes().contains(&0) {
            return Err(TransportError::InvalidEndpoint {
                authority: host,
                reason: "host cannot contain a NUL byte",
            });
        }
        if port == 0 {
            return Err(TransportError::InvalidEndpoint {
                authority: host,
                reason: "port cannot be zero",
            });
        }
        Ok(Self { host, port })
    }

    /// Parses `host:port` or bracketed `[IPv6]:port` realm-list syntax.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::InvalidEndpoint`] when the authority is
    /// missing a host or decimal nonzero port, or uses ambiguous unbracketed IPv6.
    pub fn parse(authority: &str) -> Result<Self, TransportError> {
        let (host, port_text) = if let Some(rest) = authority.strip_prefix('[') {
            let (host, suffix) =
                rest.split_once(']')
                    .ok_or_else(|| TransportError::InvalidEndpoint {
                        authority: authority.to_owned(),
                        reason: "bracketed IPv6 host is missing its closing bracket",
                    })?;
            let port = suffix
                .strip_prefix(':')
                .ok_or_else(|| TransportError::InvalidEndpoint {
                    authority: authority.to_owned(),
                    reason: "bracketed IPv6 host is missing its port separator",
                })?;
            (host, port)
        } else {
            let (host, port) =
                authority
                    .rsplit_once(':')
                    .ok_or_else(|| TransportError::InvalidEndpoint {
                        authority: authority.to_owned(),
                        reason: "authority must contain a port separator",
                    })?;
            if host.contains(':') {
                return Err(TransportError::InvalidEndpoint {
                    authority: authority.to_owned(),
                    reason: "IPv6 hosts must be enclosed in brackets",
                });
            }
            (host, port)
        };
        let port = port_text
            .parse::<u16>()
            .map_err(|_| TransportError::InvalidEndpoint {
                authority: authority.to_owned(),
                reason: "port must be a decimal value from 1 through 65535",
            })?;
        Self::new(host, port).map_err(|error| match error {
            TransportError::InvalidEndpoint { reason, .. } => TransportError::InvalidEndpoint {
                authority: authority.to_owned(),
                reason,
            },
            _ => unreachable!("endpoint construction returns only validation errors"),
        })
    }

    /// Returns the DNS name or IP literal without IPv6 brackets.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the TCP port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for TcpEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(formatter, "[{}]:{}", self.host, self.port)
        } else {
            write!(formatter, "{}:{}", self.host, self.port)
        }
    }
}
