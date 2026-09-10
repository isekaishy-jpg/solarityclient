//! Retired task failures cannot escape into a replacement world's admission.

use super::{
    PendingGeneration, ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectPresentation,
    RuntimeGameObjectResourceKind,
};
use crate::test_support::ClientFixture;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, AssetStoreHandle, ClientDataRoot,
    GameObjectDisplayCatalog, Locale,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn fishing_hole_opacity_follows_model_admission_and_independent_lifetimes()
-> Result<(), Box<dyn Error>> {
    use crate::test_support::game_object_models as models;
    use glam::Vec3;
    use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldTransform};
    let fixture = ClientFixture::with_common_files(&[
        (
            "World\\GameObject.m2",
            &models::model_with_animations(&[0])?,
        ),
        ("World\\GameObject00.skin", &models::skin()?),
        (
            "DBFilesClient\\GameObjectDisplayInfo.dbc",
            &models::displays(),
        ),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut owner =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for (guid, dynamic) in [(20, 0), (30, 2)] {
        world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(Vec3::ZERO, 0.)),
            [],
        )?;
        solarity_systems::project_object_fields(
            &mut world,
            guid,
            [
                (4, 1),
                (5, 1_f32.to_bits()),
                (8, 42),
                (14, dynamic),
                (17, 31 << 8 | 1),
            ],
        )?;
    }
    let mut random = crate::random::CrtRand::new();
    owner.synchronize(Some(&world))?;
    owner.synchronize_animations(Some(&world), &mut random)?;
    let first = std::rc::Rc::clone(owner.instances[0].opacity_owner());
    let second = std::rc::Rc::clone(owner.instances[1].opacity_owner());
    assert_eq!(first.opacity(), 0.);
    assert_eq!(second.opacity(), 0.);
    owner
        .frame_input(Some(&world))
        .advance_scene(500., &mut random)?;
    assert!((first.opacity() - 64. / 255.).abs() < 1e-6);
    assert!((second.opacity() - 127. / 255.).abs() < 1e-6);
    // Field changes cannot replay the model's admission callback.
    solarity_systems::project_object_fields(&mut world, 20, [(14, 2)])?;
    owner.synchronize(Some(&world))?;
    owner.synchronize_animations(Some(&world), &mut random)?;
    owner
        .frame_input(Some(&world))
        .advance_scene(1000., &mut random)?;
    assert!((first.opacity() - 128. / 255.).abs() < 1e-6);
    assert_eq!(second.opacity(), 1.);
    // A changed display reselects the target on the retained object owner.
    solarity_systems::project_object_fields(&mut world, 20, [(8, 43)])?;
    owner.synchronize(Some(&world))?;
    owner.synchronize_animations(Some(&world), &mut random)?;
    owner
        .frame_input(Some(&world))
        .advance_scene(1500., &mut random)?;
    assert!((first.opacity() - 191. / 255.).abs() < 1e-6);
    Ok(())
}

#[test]
fn game_object_task_panics_only_fail_the_owning_world() -> Result<(), Box<dyn Error>> {
    let mut displays = b"WDBC".to_vec();
    for word in [0_u32, 19, 76, 1] {
        displays.extend_from_slice(&word.to_le_bytes());
    }
    displays.push(0);
    let fixture = ClientFixture::with_common_files(&[(
        "DBFilesClient\\GameObjectDisplayInfo.dbc",
        &displays,
    )])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut owner =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    for retired in [false, true] {
        let task = cpu.try_submit(|| panic!("injected GameObject worker failure"))?;
        owner.pending = Some(PendingGeneration {
            request: ResourceRequest {
                kind: RuntimeGameObjectResourceKind::M2,
                path: AssetPath::new("World\\Failed.m2")?,
            },
            eligible: true,
            task,
        });
        if retired {
            owner.disconnect();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !owner
            .pending
            .as_ref()
            .is_some_and(|pending| pending.task.is_finished())
        {
            if Instant::now() >= deadline {
                return Err("injected worker did not finish".into());
            }
            std::thread::yield_now();
        }
        let result = owner.finish_pending();
        if retired {
            result?;
        } else {
            assert!(matches!(
                result,
                Err(RuntimeGameObjectError::Cpu(CpuError::TaskPanicked))
            ));
        }
        assert!(owner.pending.is_none());
    }
    cpu.shutdown()?;
    Ok(())
}
