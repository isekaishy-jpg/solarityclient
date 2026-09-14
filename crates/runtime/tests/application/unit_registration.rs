//! Shared registration must match fresh native queries through scene changes.

use crate::test_support::registration_fixture as fixture;

use super::*;
use crate::test_support::{ClientFixture, game_object_models};
use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale, MapCatalog,
};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};

#[test]
fn shared_unit_consumers_preserve_native_floor_and_invalidate_removed_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let (root, group, wdt, map) = fixture::fixture_files();
    let files = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map),
        ("World\\Maps\\Light\\Light.wdt", &wdt),
        ("World\\Light.wmo", &root),
        ("World\\Light_000.wmo", &group),
        ("Receiver.m2", &game_object_models::model()?),
        ("Receiver00.skin", &game_object_models::skin()?),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(files.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain =
        super::super::super::RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Light",
        Vec3::ZERO,
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    let transform = terrain
        .resident_m2_scene()
        .ok_or("doodad scene")?
        .placements()[0]
        .transform();
    let position = transform.transform_point3(Vec3::new(1., 1., 0.));
    let active = terrain.active.as_mut().ok_or("resident map")?;
    let native = active.query_unit_registration(position)?;
    assert!(
        native
            .selected()
            .is_some_and(|candidate| candidate.hit().is_interior())
    );
    for _ in 0..4 {
        assert_eq!(active.unit_registration(position)?, native);
    }
    // Every coordinate matters, including vertical movement and leaving the root.
    for delta in [Vec3::Z, -Vec3::Z, Vec3::X * 100.] {
        let point = position + delta;
        let expected = active.query_unit_registration(point)?;
        assert_eq!(active.unit_registration(point)?, expected);
        assert_eq!(active.unit_registration(point)?, expected);
    }
    for point in [Vec3::splat(f32::NAN), Vec3::splat(f32::INFINITY)] {
        assert!(active.unit_registration(point).is_err());
        assert!(active.unit_registration(point).is_err());
    }
    // Remove the published root using the production membership invalidation.
    active.global_world_model = None;
    active.synchronize_movement_owners();
    let expected = active.query_unit_registration(position)?;
    assert!(expected.selected().is_none());
    assert_eq!(active.unit_registration(position)?, expected);
    Ok(())
}
