//! Warden challenge and integrity-protocol state.
//!
//! `WardenClient.cpp` establishes that this is a distinct stock network
//! responsibility. The implementation must model the observed protocol and
//! must never weaken, bypass, or silently acknowledge unsupported challenges.

mod warden_client;
