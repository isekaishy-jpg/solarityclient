//! One owned model's phase order keeps shadow-only work separate from effects.

use super::super::super::{RuntimeTerrainFrameError, advance_ribbons};
use super::{GeometryContext, GeometryJob};

impl GeometryJob {
    /// All complete palettes and draw-material packets execute on a CPU worker.
    /// Ordered CPU callbacks have already consumed their independently sampled bones.
    pub(super) fn prepare(
        &mut self,
        context: &GeometryContext,
        job_context: &solarity_cpu::JobContext<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(input) = self.input else {
            unreachable!("admitted draw job owns its inputs");
        };
        let source = &context.source;
        input.trace.link("m2.geometry.execute");
        let _trace = input.trace.enter();
        let mut profile = solarity_profiling::detail_profile!("m2.geometry");
        profile.trace_owner(
            input.placement_index as u64 + 1,
            input.source_index as u64 + 1,
        );
        if self.palette.pending {
            self.pose.recompose_with_overrides(
                source.model.animations(),
                input.clock,
                input.model_view,
                self.palette
                    .overrides(&source.model_oriented_billboard_bones),
            )?;
        }
        profile.mark("palette");
        self.prepare_shadows(source, input)?;
        profile.mark("shadow packets");
        let Some(visible) = input.visible else {
            return Ok(());
        };
        self.prepare_particles(context, input, visible, job_context)?;
        profile.mark("particle simulation and geometry");
        advance_ribbons(
            &source.model,
            input.transform,
            &mut self.ribbons,
            &self.pose,
            input.clock,
            visible.effect_delta_seconds,
            context.effect_scale,
            visible.instance_color.w,
        )?;
        profile.mark("ribbon simulation");
        self.prepare_meshes(context, input, visible)?;
        profile.mark("material and mesh packets");
        self.prepare_ribbon_draws(context, input, visible)?;
        Ok(())
    }
}
