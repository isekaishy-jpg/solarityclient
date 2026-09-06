//! One wrapping client clock for event dispatch, movement, and time synchronization.

/// Uses the same SDL epoch as source event timestamps after conversion to ms.
pub(crate) fn client_milliseconds() -> u32 {
    sdl3::timer::ticks() as u32
}
