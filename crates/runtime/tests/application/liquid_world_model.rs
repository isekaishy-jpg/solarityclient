//! Native WMO factory selection through decoded root, group and DBC records.

use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelCache;
use std::error::Error;

use solarity_asset::{AssetPath, BlpTextureCache};
use solarity_rendering::LiquidDepthCoordinates;

use crate::application::liquid::{
    LiquidAssetCache, ResidentLiquidSurface, WorldModelLiquidLighting,
};
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelSource;
use crate::test_support::{ClientFixture, liquid_models};

/// Shared references reuse one worker result, while a retired decoded
/// generation cannot lend its mesh to a later load of the same virtual path.
#[test]
fn world_model_worker_mesh_shares_and_retires_with_its_generation() -> Result<(), Box<dyn Error>> {
    use std::sync::Arc;
    let files = liquid_models::files(0, 0, 0, 1, 0xa1234567);
    let fixture = ClientFixture::with_common_files(
        &files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>(),
    )?;
    let mut store = super::mounted(&fixture)?;
    let path = AssetPath::new("World\\Liquid.wmo")?;
    let mut cache = ResidentWorldModelCache::default();
    let mut textures = BlpTextureCache::default();
    let mut liquids = LiquidAssetCache::default();
    let first =
        ResidentWorldModelSource::load(&path, &mut cache, &mut textures, &mut liquids, &mut store)?;
    let second =
        ResidentWorldModelSource::load(&path, &mut cache, &mut textures, &mut liquids, &mut store)?;
    assert!(Arc::ptr_eq(first.model(), second.model()));
    assert!(Arc::ptr_eq(first.plan(), second.plan()));
    let eager = solarity_rendering::WorldModelMeshPlan::prepare(first.model())?;
    assert_eq!(first.plan().vertex_bytes(), eager.vertex_bytes());
    assert_eq!(first.plan().index_bytes(), eager.index_bytes());
    assert_eq!(first.plan().draws(), eager.draws());
    assert_eq!(first.plan().shadow_draws(), eager.shadow_draws());
    assert_eq!(cache.collect_unused(), 0);
    let old_plan = Arc::clone(first.plan());
    drop(first);
    drop(second);
    assert_eq!(cache.collect_unused(), 1);
    let replacement =
        ResidentWorldModelSource::load(&path, &mut cache, &mut textures, &mut liquids, &mut store)?;
    assert!(!Arc::ptr_eq(&old_plan, replacement.plan()));
    assert_eq!(old_plan.vertex_bytes(), replacement.plan().vertex_bytes());
    let released = Arc::downgrade(replacement.plan());
    drop(replacement);
    assert!(
        released.upgrade().is_none(),
        "cache must not retain an unused mesh"
    );
    assert_eq!(cache.collect_unused(), 1);
    Ok(())
}

/// All 224 expected factory choices come from original 793D20 instructions.
#[test]
fn world_model_liquid_factory_matches_original_material_tint_and_lighting()
-> Result<(), Box<dyn Error>> {
    let bytes = include_bytes!("../fixtures/liquid_wmo_material.bin");
    assert_eq!(bytes.len(), 224 * 44);
    for (case, record) in bytes.as_chunks::<44>().0.iter().enumerate() {
        let words = record
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(u32::from_le_bytes)
            .collect::<Vec<_>>();
        let files = liquid_models::files(words[0], words[1], words[2], words[3], 0xa1234567);
        let fixture = ClientFixture::with_common_files(
            &files
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
                .collect::<Vec<_>>(),
        )?;
        let mut store = super::mounted(&fixture)?;
        let mut materials = LiquidAssetCache::default();
        let source = ResidentWorldModelSource::load(
            &AssetPath::new("World\\Liquid.wmo")?,
            &mut ResidentWorldModelCache::default(),
            &mut BlpTextureCache::default(),
            &mut materials,
            &mut store,
        )?;
        assert_eq!(source.liquids().len(), 1);
        let batch = &source.liquids()[0];
        let ResidentLiquidSurface::Authored(surface) = &batch.material.surfaces[0] else {
            return Err("authored fixture image missing".into());
        };
        assert_eq!(
            surface.path(),
            &AssetPath::new(format!("XTextures\\type{}.blp", words[4]))?,
            "material case {case}"
        );
        assert_eq!(
            batch.material.authored_surface_coordinates,
            words[7] != 0,
            "UV mode case {case}"
        );
        assert_eq!(
            matches!(batch.lighting, WorldModelLiquidLighting::Interior),
            words[9] != 0,
            "lighting case {case}"
        );
        let vertex = batch.mesh.vertices()[0];
        let bytes = vertex.to_bytes();
        let tint = words[5];
        assert_eq!(
            &bytes[24..28],
            &[
                (tint >> 16) as u8,
                (tint >> 8) as u8,
                tint as u8,
                (tint >> 24) as u8
            ],
            "color case {case}"
        );
        if words[7] == 0 {
            assert_eq!(
                vertex.depth_coordinates()[0].to_bits(),
                words[8],
                "depth column case {case}"
            );
            let bank = if words[10] == 13 {
                LiquidDepthCoordinates::River
            } else {
                LiquidDepthCoordinates::Ocean
            };
            assert_eq!(
                vertex.depth_coordinates()[1].to_bits(),
                if words[10] == 23 {
                    0
                } else {
                    bank.coordinate(17).to_bits()
                },
                "original group depth bank survives material remap case {case}"
            );
        } else {
            assert_eq!(vertex.surface_coordinates(), [529. / 256., 1027. / 256.]);
        }
        assert_eq!(batch.mesh.indices(), &[0, 0, 2, 1, 3, 3]);
    }
    Ok(())
}
