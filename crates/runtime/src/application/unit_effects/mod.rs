//! Ordered unit registration and shared, resumable authored effect sources.

mod sources;
mod state;

pub(super) use state::RuntimeUnitEffects;

#[cfg(test)]
pub(in crate::application) use sources::prepare_sources_for_test;
