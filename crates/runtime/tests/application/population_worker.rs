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
use std::sync::mpsc::{Sender, channel};
use std::thread::{self, ThreadId};

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
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
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
