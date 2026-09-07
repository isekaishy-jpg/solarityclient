//! Opt-in application timing, reported once per scope every two seconds.

mod aggregate;
mod frame;

pub(super) use frame::RuntimeFrameProfile;
