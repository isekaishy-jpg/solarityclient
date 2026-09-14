//! Infrequent OS counters include native mixer and driver threads outside Rust scopes.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use windows::{thread_id, write_snapshot};

/// Non-Windows builds explicitly report that OS resource sampling is unavailable.
#[cfg(not(windows))]
pub(crate) fn write_snapshot(
    output: &mut impl std::io::Write,
    seconds: f64,
) -> std::io::Result<()> {
    writeln!(output, "{seconds:.6},unavailable,0,0,0,0,0,0,0,0")
}

/// Rust's thread identifier remains in shard names on unsupported hosts.
#[cfg(not(windows))]
pub(crate) fn thread_id() -> u32 {
    0
}
