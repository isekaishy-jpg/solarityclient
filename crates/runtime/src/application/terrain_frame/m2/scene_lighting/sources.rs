//! One retained light bank is shared only after ordered source publication ends.

use solarity_rendering::{M2DirectionalLight, ScenePointLights};
use std::sync::Arc;

/// Immutable inputs for terrain/liquid queries while receiver uniforms execute.
#[derive(Clone, Copy)]
pub(in crate::application::terrain_frame) struct SceneLightInputs<'frame> {
    pub trace: solarity_profiling::TraceContext,
    pub points: &'frame ScenePointLights,
    pub directionals: &'frame [M2DirectionalLight],
}

/// The frame retains allocation ownership; a worker pins this exact bank until
/// reclamation. Receiver-specific exterior light never mutates these sources.
#[derive(Default)]
pub(super) struct SceneLightSources {
    pub trace: solarity_profiling::TraceContext,
    pub points: ScenePointLights,
    pub directionals: Vec<M2DirectionalLight>,
}

impl SceneLightSources {
    /// Mutation follows reclamation, never copy-on-write of an in-flight frame.
    pub(super) fn exclusive(owner: &mut Arc<Self>) -> &mut Self {
        Arc::get_mut(owner)
            .unwrap_or_else(|| unreachable!("scene light readers return before source mutation"))
    }
}
