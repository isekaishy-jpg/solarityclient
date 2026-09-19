//! Both shadow collectors share one immutable packet result per model.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError, shadow};
use super::{GeometryInput, GeometryJob};

impl GeometryJob {
    /// Material sampling is independent of gameplay callbacks and reuses the visible
    /// pass's scratch. Shadow-only jobs never own or advance particle/ribbon state.
    pub(super) fn prepare_shadows(
        &mut self,
        source: &M2GpuSource,
        input: GeometryInput,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !input.primary_shadow && input.environment_maps == 0 {
            return Ok(());
        }
        shadow::append_packets(source, input.shadow, &mut self.material_poses, |draw| {
            self.shadow_draws.push(draw)?;
            Ok(())
        })
    }
}
