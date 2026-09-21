//! Owned GameObject source work and resumable root/doodad dependency consumption.
mod state;
mod world;
pub(super) use state::{
    GameObjectM2Input, GameObjectWorkerCompletion, GameObjectWorkerSource, GameObjectWorkerState,
    prepare_on_worker,
};
pub(super) use world::world_model_steps;

#[cfg(test)]
#[path = "../../../../tests/application/game_object_world_steps.rs"]
mod tests;
