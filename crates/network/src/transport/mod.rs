//! Tokio-backed TCP connection, buffered I/O, and connection timing.
//!
//! `OsTcp.cpp` and the socket imports provide the stock platform evidence.
//! This module exposes transport outcomes without inventing retries that the
//! target client does not perform.

mod os_tcp;
