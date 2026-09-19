//! Numeric main readiness preserves ordered world owners and narrow light consumers.

use crate::application::frame_pipeline::FrameWait;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use solarity_cpu::{
    CompletionPort, CompletionProducer, CpuError, CpuExecutor, CpuStorageClass, JobOutcome,
    MainReadyQueue, ReadyToken,
};

/// Runtime retains the operation vocabulary; CPU sees only its numeric identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub(super) enum MainPreparationStep {
    GroundDetail,
    WorldModels,
    Surfaces,
    Uniforms,
}

/// Retained storage for the whole world's main continuation phase. The two
/// ports represent completed ground selection and published model light sources.
pub(in crate::application::terrain_frame) struct WorldMainPreparation {
    queue: MainReadyQueue,
    ground: CompletionPort,
    lights: CompletionPort,
}

impl WorldMainPreparation {
    /// Creates closed reusable products and shares the existing native wake bridge.
    pub(super) fn new(cpu: &CpuExecutor) -> Result<Self, CpuError> {
        let ground = CompletionPort::new(2, cpu.storage(), CpuStorageClass::Frame)?;
        let lights = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
        // Initial ports have no frame owner. Close them so begin always uses the
        // same restart/producer ownership path, including after failed admission.
        drop(ground.producer()?);
        drop(lights.producer()?);
        Ok(Self {
            queue: cpu.main_ready_queue(),
            ground,
            lights,
        })
    }

    /// Reserves every main node/edge before M2 transfers frame input to workers.
    pub(super) fn begin(&mut self, cpu: &CpuExecutor) -> Result<WorldMainFrame<'_>, CpuError> {
        self.queue
            .begin(4, 4, cpu.storage(), CpuStorageClass::Frame)?;
        let mut frame = WorldMainFrame {
            owner: self,
            ground: None,
            lights: None,
            sources_published: false,
            uniforms_ready: false,
            initial_done: false,
        };
        frame
            .owner
            .ground
            .restart(2, cpu.storage(), CpuStorageClass::Frame)?;
        frame.ground = Some(frame.owner.ground.producer()?);
        frame
            .owner
            .lights
            .restart(1, cpu.storage(), CpuStorageClass::Frame)?;
        frame.lights = Some(frame.owner.lights.producer()?);
        frame
            .owner
            .queue
            .watch(MainPreparationStep::GroundDetail as u64, &[])?;
        frame.owner.queue.watch(
            MainPreparationStep::WorldModels as u64,
            &[frame.owner.ground.readiness()],
        )?;
        frame.owner.queue.watch(
            MainPreparationStep::Surfaces as u64,
            &[
                frame.owner.ground.readiness(),
                frame.owner.lights.readiness(),
            ],
        )?;
        Ok(frame)
    }
}

/// Dropping an incomplete driver withdraws numeric subscriptions before dropping
/// its external producers. Domain payloads remain in their existing scoped owners.
pub(super) struct WorldMainFrame<'a> {
    owner: &'a mut WorldMainPreparation,
    ground: Option<CompletionProducer>,
    lights: Option<CompletionProducer>,
    sources_published: bool,
    uniforms_ready: bool,
    initial_done: bool,
}

impl WorldMainFrame<'_> {
    /// FIFO applies to readiness; explicit dependency edges preserve stock order.
    pub(super) fn take_ready(&mut self) -> Option<(MainPreparationStep, JobOutcome)> {
        let notice = self.owner.queue.take_ready()?;
        let step = match notice.key() {
            0 => MainPreparationStep::GroundDetail,
            1 => {
                self.initial_done = true;
                MainPreparationStep::WorldModels
            }
            2 => MainPreparationStep::Surfaces,
            3 => {
                self.uniforms_ready = true;
                MainPreparationStep::Uniforms
            }
            _ => unreachable!("world registers only its four operation identities"),
        };
        Some((step, notice.outcome()))
    }

    /// Failure skips dependent preparation, while its original error remains on main.
    pub(super) fn ground_finished(&mut self, succeeded: bool) -> Result<(), CpuError> {
        let mut producer = self.ground.take().ok_or(CpuError::ReadinessConflict)?;
        producer.complete(if succeeded {
            JobOutcome::Succeeded
        } else {
            JobOutcome::Failed
        })?;
        Ok(())
    }

    /// Terrain/liquid need published sources, while final M2 consumption needs
    /// receiver computation. Neither edge authorizes callbacks to run again.
    pub(super) fn publish_sources(&mut self, uniforms: Option<ReadyToken>) -> Result<(), CpuError> {
        if self.sources_published {
            return Ok(());
        }
        let mut lights = self.lights.take().ok_or(CpuError::ReadinessConflict)?;
        lights.complete(JobOutcome::Succeeded)?;
        self.owner
            .queue
            .watch(MainPreparationStep::Uniforms as u64, uniforms.as_slice())?;
        self.sources_published = true;
        Ok(())
    }

    pub(super) fn needs_uniform_notice(&self) -> bool {
        self.sources_published && !self.uniforms_ready
    }

    pub(super) fn initial_done(&self) -> bool {
        self.initial_done
    }
    pub(super) fn sources_published(&self) -> bool {
        self.sources_published
    }

    /// This is selected only with an admitted receiver prerequisite and no ready
    /// main work. Terminal batch reclamation remains with the exact M2 owner.
    pub(super) fn wait(&self, wait: &mut FrameWait<'_>) -> Result<(), RuntimeTerrainFrameError> {
        wait.before_continuation(&self.owner.queue)?;
        Ok(())
    }
}

impl Drop for WorldMainFrame<'_> {
    fn drop(&mut self) {
        self.owner.queue.cancel();
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/world_main_continuations.rs"]
mod tests;
