//! CEffect completion mutates the same primary owned by the scene callback.

use super::*;
use crate::test_support::{ClientFixture, game_object_models};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, ClientDataRoot, DecodedM2Model, Locale,
};
use std::error::Error;

#[test]
fn completion_uses_authored_despawn_or_pins_the_terminal_pose() -> Result<(), Box<dyn Error>> {
    for despawn in [false, true] {
        let ids: &[u16] = if despawn { &[0, 159] } else { &[0] };
        let bytes = game_object_models::model_with_animations(ids)?;
        let mut dbc = b"WDBC".to_vec();
        for word in [1_u32, 8, 32, 1, 0, 0, 0, 0, 0, 0, 0, 0] {
            dbc.extend_from_slice(&word.to_le_bytes());
        }
        dbc.push(0);
        let fixture = ClientFixture::with_common_files(&[
            ("World\\GameObject.m2", &bytes),
            ("World\\GameObject00.skin", &game_object_models::skin()?),
            ("DBFilesClient\\AnimationData.dbc", &dbc),
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = DecodedM2Model::load(&mut store, &AssetPath::new("World\\GameObject.m2")?)?;
        let animations = AnimationDataCatalog::load(&mut store)?;
        let mut random = CrtRand::new();
        let mut playback = M2Playback::unit_effect_default_sequence(
            &model,
            &animations,
            10_000,
            10_000,
            M2SequenceStartPhase::DuringSceneUpdate,
            &mut random,
        )?;
        let mut phase = UnitEffectPhase::Playing;
        let end = playback.script_timer.ok_or("initial timer")?.end_time_ms();
        phase.advance(&mut playback, &model, (end - 2) as f32, &mut random)?;
        assert_eq!(phase, UnitEffectPhase::Playing);
        let original_random = random;
        phase.advance(&mut playback, &model, (end + 1) as f32, &mut random)?;
        if despawn {
            assert_eq!(phase, UnitEffectPhase::Despawning);
            assert_eq!(playback.animation_id, 159);
            let mut expected_random = original_random;
            let _ = expected_random.next_u15();
            let _ = expected_random.next_u15();
            assert_eq!(random, expected_random);
            let end = playback.script_timer.ok_or("despawn timer")?.end_time_ms();
            phase.advance(&mut playback, &model, (end + 1) as f32, &mut random)?;
            assert_eq!(phase, UnitEffectPhase::Retiring);
        } else {
            assert_eq!(phase, UnitEffectPhase::Retiring);
            assert_eq!(random, original_random);
            let clock = phase
                .advance(&mut playback, &model, (end + 5000) as f32, &mut random)?
                .clock;
            assert_eq!(clock.animation_time_ms(), 999.);
            assert_eq!(random, original_random);
        }
        // A positioned effect belongs to the unit even without a model parent.
        let world = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
            solarity_ecs::WorldMapId::new(0),
            1,
            "Effect owner",
            Vec3::ZERO,
            0.,
        ));
        let lifetime = Rc::new(());
        let mut placement = m2_gpu_placement(
            0,
            Mat4::IDENTITY,
            M2GpuPlacementOwner::UnitEffect { serial: 0 },
            &model,
            Some(M2PlaybackStorage::Local(playback)),
            None,
            0,
        )?;
        placement.unit_effect = Some(UnitEffectPlacement {
            identity: world.object_identity(1).ok_or("unit identity")?,
            lifetime: Rc::downgrade(&lifetime),
            kind: UnitWaterEffect::RunSpray.into(),
            kit: None,
            sound_entry: 0,
            definition_id: 1,
            sound_lifetime: None,
            binding: UnitEffectBinding::Positioned {
                position: Vec3::ZERO,
                world_factor: 1.,
                unit_scale: 1.,
            },
            scale: UnitEffectScale {
                multiplier: 1.,
                minimum: 0.,
                maximum: 10.,
            },
            phase: UnitEffectPhase::Playing,
        });
        let mut scene = M2UnitEffectScene::default();
        scene.prepare_attachment(&mut placement);
        assert!(!placement.unit_effect.as_ref().ok_or("effect")?.retiring());
        drop(lifetime);
        scene.prepare_attachment(&mut placement);
        assert!(placement.unit_effect.as_ref().ok_or("effect")?.retiring());
        // Dirty topology scans from zero; current topology can skip the
        // ordinary prefix. Both retain a later ordinary placement in order.
        let ordinary = |guid| {
            m2_gpu_placement(
                0,
                Mat4::IDENTITY,
                M2GpuPlacementOwner::CreatureBody { guid },
                &model,
                None,
                None,
                0,
            )
        };
        let mut placements = vec![ordinary(10)?, placement, ordinary(20)?];
        let first_effect = usize::from(despawn);
        scene
            .anchors
            .insert(1, HashMap::from([(17, Some(Mat4::IDENTITY))]));
        assert!(scene.retire_drained(&mut placements, first_effect));
        assert_eq!(
            placements.iter().map(|p| p.owner).collect::<Vec<_>>(),
            [
                M2GpuPlacementOwner::CreatureBody { guid: 10 },
                M2GpuPlacementOwner::CreatureBody { guid: 20 },
            ]
        );
        assert!(scene.anchors.is_empty());
        let no_effects = placements.len();
        assert!(!scene.retire_drained(&mut placements, no_effects));
    }
    Ok(())
}
