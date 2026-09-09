//! Non-consuming closure observation for an idle authenticated character screen.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use tokio::io::Interest;
use tokio::net::TcpStream;

use super::{WorldSessionError, WorldSessionStage};
use crate::connection::WorldSession;

impl WorldSession<TcpStream> {
    /// Observes remote closure without consuming an encrypted packet header.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the transport readiness query fails.
    pub fn connection_closed(&self) -> Result<bool, WorldSessionError> {
        connection_closed(&self.stream).map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Receive,
            message: error.to_string(),
        })
    }
}

/// Polling readiness is cancellation-safe and leaves buffered packet bytes intact.
fn connection_closed(stream: &TcpStream) -> std::io::Result<bool> {
    let mut ready = pin!(stream.ready(Interest::READABLE));
    match ready.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(Ok(ready)) => Ok(ready.is_read_closed() || ready.is_error()),
        Poll::Ready(Err(error)) => Err(error),
        Poll::Pending => Ok(false),
    }
}

#[cfg(test)]
#[path = "../../tests/session/connection_state.rs"]
mod tests;
