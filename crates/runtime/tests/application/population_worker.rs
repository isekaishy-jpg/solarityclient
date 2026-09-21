//! Exact object lifetime rejection and worker-owned retirement during GPU admission.

use super::{
    PendingPopulation, PopulationStage, PopulationWorker, PreparedPopulation,
    UnitPresentationGeneration,
};
use glam::Vec3;
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
use solarity_rendering::CharacterComponentTextureLevel;
use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::thread::{self, ThreadId};
use std::time::Duration;

impl super::super::RuntimePlayerPresentation {
    /// A withdrawn NPC must return its bank before a replacement lifetime can join.
    pub(in crate::application) fn creature_preparation_pending(&self) -> bool {
        self.creature_worker
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.stage.is_finished())
    }

    /// Local fixtures distinguish source/CPU readiness from main-only GPU warmup.
    pub(in crate::application) fn local_preparation_pending(&self) -> bool {
        self.local_worker
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.stage.is_finished())
    }

    /// Tests wait on the real coordinator notifier only while CPU work remains.
    /// This observes readiness without consuming results or bypassing GPU admission.
    pub(in crate::application) fn population_preparation_pending(&self) -> bool {
        self.creature_worker
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.stage.is_finished())
            || self
                .remote_worker
                .pending
                .as_ref()
                .is_some_and(|pending| !pending.stage.is_finished())
    }
}

struct PreparedProbe {
    generation: UnitPresentationGeneration,
    dropped: Sender<ThreadId>,
}

impl PreparedPopulation for PreparedProbe {
    fn generation(&self) -> &UnitPresentationGeneration {
        &self.generation
    }
}

impl Drop for PreparedProbe {
    fn drop(&mut self) {
        let _ = self.dropped.send(thread::current().id());
    }
}

/// Controlled completion wakes the test through the runtime notifier contract.
struct PopulationNotifier(Sender<()>);

impl solarity_cpu::CoordinatorNotifier for PopulationNotifier {
    fn notify(&self) {
        let _ = self.0.send(());
    }
}

#[test]
fn withdrawn_attempt_reclaims_failure_even_when_its_object_is_still_current()
-> Result<(), Box<dyn std::error::Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    world.create_object(9, ObjectKind::Unit, None, [])?;
    let identity = world.object_identity(9).ok_or("missing identity")?;
    let (notify, ready) = channel();
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::MIN,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ),
        Arc::new(PopulationNotifier(notify)),
    )?;
    let mut worker = PopulationWorker::<u32, PreparedProbe>::new();
    let cache = worker.cache.take().ok_or("owned cache bank")?;
    let (release, blocked) = channel();
    let task = cpu.try_reserve()?.submit(move || {
        // The consumer withdraws before this cancellation is published.
        let result = blocked
            .recv()
            .map_err(|_| solarity_cpu::CpuError::CompletionLost)
            .and(Err(solarity_cpu::CpuError::JobCancelled))
            .map_err(Into::into);
        super::super::worker_presentation::AppearanceCompletion { cache, result }
    });
    worker.pending = Some(PendingPopulation {
        identity,
        key: 17,
        level: CharacterComponentTextureLevel::DEFAULT,
        withdrawn: false,
        stage: PopulationStage::Preparing(
            super::super::worker_presentation::AppearanceTask::Direct { task, demand: None },
        ),
    });
    worker.discard_ready(identity)?;
    assert!(worker.pending.is_some());
    release.send(())?;
    while worker
        .pending
        .as_ref()
        .is_some_and(|pending| !pending.stage.is_finished())
    {
        ready.recv_timeout(Duration::from_secs(10))?;
    }
    // Exact identity still matches. The next frame must nevertheless discard the
    // withdrawn failure before trying to consume or readmit the same appearance.
    worker.service(Some(&world), &cpu)?;
    assert!(worker.pending.is_none());
    assert!(worker.cache.is_some());
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn reused_guid_retires_warming_generation_on_cpu_even_when_initially_saturated()
-> Result<(), Box<dyn std::error::Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    world.create_object(9, ObjectKind::Unit, None, [])?;
    let identity = world.object_identity(9).ok_or("missing identity")?;
    let (sender, receiver) = channel();
    let mut worker = PopulationWorker::new();
    worker.pending = Some(PendingPopulation {
        identity,
        key: 17,
        level: CharacterComponentTextureLevel::DEFAULT,
        withdrawn: false,
        stage: PopulationStage::Warming {
            resident: PreparedProbe {
                generation: UnitPresentationGeneration::new(),
                dropped: sender,
            },
            pipelines: VecDeque::new(),
        },
    });
    world.remove_object(9)?;
    world.create_object(9, ObjectKind::Unit, None, [])?;
    let replacement = world.object_identity(9).ok_or("missing replacement")?;
    assert_ne!(identity, replacement);
    assert!(!worker.accepts(replacement));
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let occupied = cpu.try_reserve()?;
    worker.service(Some(&world), &cpu)?;
    assert!(worker.accepts(replacement));
    assert!(
        receiver.try_recv().is_err(),
        "saturation cannot free the atlas inline"
    );
    drop(occupied);
    worker.service(Some(&world), &cpu)?;
    cpu.shutdown()?;
    assert_ne!(receiver.try_recv()?, thread::current().id());
    assert!(receiver.try_recv().is_err());
    Ok(())
}
