//! Main-owned Glue selection, dependency admission and stock appearance construction.

mod coordinator;
mod pending;
mod residency;
mod source;

pub(super) use pending::PendingGlueCharacter;
pub(super) use source::GlueModelSource;

#[cfg(test)]
#[path = "../../../../tests/application/glue_character_requests.rs"]
mod tests;
