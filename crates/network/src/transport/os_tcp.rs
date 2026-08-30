//! Tokio implementation of the socket ownership recovered from `OsTcp.cpp`.

use tokio::net::TcpStream;

use super::{TcpEndpoint, TransportError};

/// Stateless owner of the operating-system TCP connection operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TcpTransport;

impl TcpTransport {
    /// Opens one normal TCP connection and enables low-latency packet writes.
    ///
    /// No application retry or guessed timeout is added. Callers that own a
    /// configured deadline may wrap this future with their runtime policy.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Connect`] when resolution or connection fails,
    /// or [`TransportError::Configure`] when TCP no-delay cannot be enabled.
    pub async fn connect(endpoint: &TcpEndpoint) -> Result<TcpStream, TransportError> {
        let authority = endpoint.to_string();
        let stream = TcpStream::connect((endpoint.host(), endpoint.port()))
            .await
            .map_err(|error| TransportError::Connect {
                endpoint: authority.clone(),
                message: error.to_string(),
            })?;
        stream
            .set_nodelay(true)
            .map_err(|error| TransportError::Configure {
                endpoint: authority,
                message: error.to_string(),
            })?;
        Ok(stream)
    }
}
