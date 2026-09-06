//! Current replicated collision uses the same resident traversal as static geometry.

#[path = "movement_interval.rs"]
mod interval;

use super::*;
use solarity_asset::GameObjectDisplayCatalog;
use solarity_ecs::{
    GameObjectMovement, GameObjectPresentation, GameObjectTransport, ObjectKind, ObjectPresentation,
};
use solarity_runtime::{
    CrtRand, RuntimeGameObjectPresentation, RuntimeMovementOwner, RuntimeMovementQuery,
    RuntimeWorldModelMovementOwner,
};
use std::sync::Arc;

struct Scene {
    _fixture: ClientFixture,
    model: Arc<solarity_asset::DecodedM2Model>,
    terrain: RuntimeTerrainCoordinator,
    objects: RuntimeGameObjectPresentation,
    world: ActiveWorld,
    random: CrtRand,
}

impl Scene {
    fn new(global: bool) -> Result<Self, Box<dyn Error>> {
        let first = TerrainTileIndex::new(21, 30).ok_or("tile")?;
        let mut wdt = Vec::new();
        if global {
            wdt = global_wmo_wdt()?;
        } else {
            let mut manifest = WdtFile::new(WowVersion::WotLK);
            manifest.mwmo = Some(MwmoChunk::new());
            for x in [21, 22] {
                manifest
                    .main
                    .get_mut(x, 30)
                    .ok_or("tile")?
                    .set_has_adt(true);
            }
            WdtWriter::new(&mut wdt).write(&manifest)?;
        }
        let mut model = m2_collision_fixture()?;
        let sequence = model.len() as u32;
        let mut stand = [0_u8; 64];
        set_u32(&mut stand, 4, 1000);
        set_u32(&mut stand, 12, 0x20);
        set_u32(&mut stand, 16, 32767);
        set_u32(&mut stand, 20, 1);
        set_u32(&mut stand, 24, 1);
        set_u16(&mut stand, 60, u16::MAX);
        model.extend_from_slice(&stand);
        set_u32(&mut model, 0x1c, 1);
        set_u32(&mut model, 0x20, sequence);
        let mut root = root_wmo_fixture();
        let modd = root
            .windows(4)
            .position(|tag| tag == b"DDOM")
            .ok_or("MODD")?
            + 8;
        set_vec3(&mut root, modd + 4, [0.25, 0.25, 0.25]);
        let mut group = group_wmo_fixture();
        let mogp = group
            .windows(4)
            .position(|tag| tag == b"PGOM")
            .ok_or("MOGP")?;
        let size = read_u32(&group, mogp + 4)?;
        // Repeated MODR entries exercise first-visit stamping independently
        // of GameObjects referenced by several neighboring MCNK lists.
        push_wmo_chunk(&mut group, *b"RDOM", &[0, 0, 0, 0]);
        set_u32(&mut group, mogp + 4, size + 12);
        let fixture = ClientFixture::with_common_files(&[
            ("DBFilesClient\\Map.dbc", &map_table()),
            ("DBFilesClient\\GameObjectDisplayInfo.dbc", &displays()),
            ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
            (
                "World\\Maps\\Northrend\\Northrend_21_30.adt",
                &streaming_adt(first, 10.)?,
            ),
            ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
            ("World\\Fixture\\Collision.m2", &model),
            ("World\\Fixture\\Collision00.skin", &skin_fixture()?),
            ("World\\Fixture\\Collision.blp", &bootstrap_texture_blp()),
            ("World\\Wmo\\Fixture.wmo", &root),
            ("World\\Wmo\\Fixture_000.wmo", &group),
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let maps = MapCatalog::load(&mut store)?;
        let displays = GameObjectDisplayCatalog::load(&mut store)?;
        let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
        let model = Arc::new(solarity_asset::DecodedM2Model::load(
            &mut store,
            &solarity_asset::AssetPath::new("World\\Fixture\\Collision.m2")?,
        )?);
        let assets = AssetStoreHandle::new(store);
        let mut terrain = RuntimeTerrainCoordinator::new(assets.clone(), maps);
        let objects = RuntimeGameObjectPresentation::new(assets, displays, animations);
        let world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(571),
            1,
            "DynamicFixture",
            if global {
                Vec3::ZERO
            } else {
                Vec3::new(1000., 5800., 50.)
            },
            0.,
        ));
        terrain.synchronize(Some(&world))?;
        Ok(Self {
            _fixture: fixture,
            model,
            terrain,
            objects,
            world,
            random: CrtRand::new(),
        })
    }

    fn add(
        &mut self,
        guid: u64,
        display: u32,
        kind: u8,
        position: Vec3,
    ) -> Result<(), Box<dyn Error>> {
        let entity = self.world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(position, 0.)),
            [],
        )?;
        self.world.storage_mut().add_component(
            entity,
            (
                ObjectPresentation::new(1, 1.),
                GameObjectPresentation::from_fields(
                    display,
                    0,
                    u32::from_le_bytes([1, kind, 0, 0]),
                )
                .with_dynamic_word(0xffff0000),
            ),
        );
        Ok(())
    }

    fn synchronize(&mut self) -> Result<RuntimeStaticMovementResidency, Box<dyn Error>> {
        self.objects.synchronize(Some(&self.world))?;
        self.objects
            .synchronize_animations(Some(&self.world), &mut self.random)?;
        Ok(self.terrain.synchronize_game_object_movement(
            Some(&self.world),
            &self.objects,
            MovementBspCacheMode::Enabled,
        )?)
    }

    fn collect(
        &mut self,
        center: Vec3,
        flags: u32,
        query: &mut RuntimeMovementQuery,
    ) -> Result<RuntimeStaticMovementResidency, Box<dyn Error>> {
        Ok(self.terrain.collect_movement(
            &self.world,
            &self.objects,
            MovementCollisionBounds::new(center - Vec3::splat(4.), center + Vec3::splat(4.))?,
            flags,
            MovementBspCacheMode::Enabled,
            query,
        )?)
    }
}

fn displays() -> Vec<u8> {
    let mut strings = vec![0];
    let mut rows = Vec::new();
    for (id, path) in [
        (42_u32, "World\\Fixture\\Collision.m2"),
        (43, "World\\Wmo\\Fixture.wmo"),
        (44, "World\\Fixture\\Collision.mdx"),
    ] {
        let mut row = [0_u32; 19];
        row[0] = id;
        row[1] = strings.len() as u32;
        strings.extend_from_slice(path.as_bytes());
        strings.push(0);
        for (slot, value) in row[12..18].iter_mut().zip([-1_f32, -1., -1., 2., 2., 2.]) {
            *slot = value.to_bits();
        }
        for value in row {
            rows.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut bytes = b"WDBC".to_vec();
    for value in [3_u32, 19, 76, strings.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend(rows);
    bytes.extend(strings);
    bytes
}

fn generic_guids(query: &RuntimeMovementQuery) -> Vec<u64> {
    (0..query.triangles().len())
        .filter_map(|index| match query.owner(index) {
            Some(RuntimeMovementOwner::GameObject { identity, .. }) => Some(identity.guid()),
            _ => None,
        })
        .collect()
}

fn assert_same_geometry(
    actual: &[solarity_systems::MovementCollisionTriangle],
    expected: &[solarity_systems::MovementCollisionTriangle],
) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.vertices(), expected.vertices());
        assert_eq!(actual.normal(), expected.normal());
    }
}

#[test]
fn generic_lists_retain_order_replace_placements_and_reject_stale_lifetimes()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new(false)?;
    let center = Vec3::new(999., 5799., 50.);
    scene.add(90, 42, 5, center)?;
    scene.add(10, 42, 5, center)?;
    assert_eq!(scene.synchronize()?, RuntimeStaticMovementResidency::Ready);
    let mut query = RuntimeMovementQuery::new();
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [10, 90],
        "head insertion and once-per-query stamping across neighboring chunks"
    );
    let first = scene.world.object_identity(90).ok_or("identity")?;
    let random = scene.random;
    for _ in 0..3 {
        scene.synchronize()?;
        scene.collect(center, 0xf00000, &mut query)?;
        assert_eq!(generic_guids(&query), [10, 90]);
    }
    assert_eq!(
        scene.random, random,
        "collision synchronization must not restart playback"
    );
    scene
        .world
        .update_transform(90, WorldTransform::new(center + Vec3::X * 0.25, 0.))?;
    scene.synchronize()?;
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [90, 10],
        "matrix changes reinsert even when destinations stay the same"
    );
    scene.world.update_fields(10, [(8, 44)])?;
    project_object_fields(&mut scene.world, 10, [(8, 44)])?;
    scene.synchronize()?;
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [10, 90],
        "display replacement creates new references even for a cached model alias"
    );
    scene.world.remove_object(90)?;
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [10],
        "callback resolves the live world before dereferencing an old list"
    );
    scene.add(90, 42, 5, center)?;
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [10],
        "a reused GUID cannot inherit a retired lifetime's references"
    );
    scene.synchronize()?;
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(generic_guids(&query), [90, 10]);
    assert_ne!(scene.world.object_identity(90), Some(first));
    scene
        .world
        .update_transform(90, WorldTransform::new(Vec3::new(999., 5299., 50.), 0.))?;
    let pending = RuntimeStaticMovementResidency::PendingTile {
        tile: TerrainTileIndex::new(22, 30).ok_or("tile")?,
    };
    assert_eq!(scene.synchronize()?, pending);
    assert_eq!(scene.collect(center, 0xf00000, &mut query)?, pending);
    assert!(query.triangles().is_empty());
    assert_eq!(query.map_id(), None);
    scene.world.update_game_object_movement(
        90,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 999,
                position: Vec3::ZERO,
                orientation: 0.,
            }),
        ),
    )?;
    assert_eq!(scene.synchronize()?, RuntimeStaticMovementResidency::Ready);
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [10],
        "unresolved placement retires references"
    );
    Ok(())
}

#[test]
fn generic_callback_applies_family_door_and_parent_guid_gates() -> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new(false)?;
    let center = Vec3::new(999., 5799., 50.);
    scene.add(90, 42, 0, center)?;
    scene.add(10, 42, 5, center)?;
    scene.add(99, 0, 5, center)?;
    scene.world.update_game_object_movement(
        10,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 99,
                position: Vec3::ZERO,
                orientation: 0.,
            }),
        ),
    )?;
    scene.synchronize()?;
    let mut query = RuntimeMovementQuery::new();
    scene.collect(center, 0xf00000, &mut query)?;
    assert_eq!(generic_guids(&query), [10, 90]);
    assert!(matches!(
        query.owner(0),
        Some(RuntimeMovementOwner::GameObject {
            reported_guid: 99,
            ..
        })
    ));
    scene.collect(center, 0xf08000, &mut query)?;
    assert_eq!(generic_guids(&query), [10], "native door query bit");
    scene.collect(center, 0x1ff, &mut query)?;
    assert!(
        generic_guids(&query).is_empty(),
        "without the high family mask the callback is never entered"
    );
    scene.add(20, 42, 5, center)?;
    scene.world.update_game_object_movement(
        20,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 99,
                position: Vec3::ZERO,
                orientation: 0.,
            }),
        ),
    )?;
    scene.synchronize()?;
    scene.collect(center, 0xf08000, &mut query)?;
    assert_eq!(
        generic_guids(&query),
        [20, 10],
        "children sharing a reported parent GUID retain separate visit stamps"
    );
    assert!((0..2).all(|index| matches!(
        query.owner(index),
        Some(RuntimeMovementOwner::GameObject {
            reported_guid: 99,
            ..
        })
    )));
    Ok(())
}

#[test]
fn replicated_root_order_survives_motion_and_retires_with_the_world() -> Result<(), Box<dyn Error>>
{
    let mut scene = Scene::new(true)?;
    scene.add(90, 43, 11, Vec3::ZERO)?;
    scene.add(10, 43, 11, Vec3::ZERO)?;
    scene.synchronize()?;
    let mut query = RuntimeMovementQuery::new();
    let root_guids = |query: &RuntimeMovementQuery| {
        (0..query.triangles().len())
            .filter_map(|index| match query.owner(index) {
                Some(RuntimeMovementOwner::GameObjectWorldModel { identity }) => {
                    Some(identity.guid())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    scene.collect(Vec3::ZERO, 0xf0, &mut query)?;
    assert_eq!(root_guids(&query), [90, 10]);
    scene
        .world
        .update_transform(90, WorldTransform::new(Vec3::X * 0.25, 0.))?;
    scene.synchronize()?;
    scene.collect(Vec3::ZERO, 0xf0, &mut query)?;
    assert_eq!(root_guids(&query), [90, 10]);
    scene.world.remove_object(90)?;
    scene.add(90, 43, 11, Vec3::ZERO)?;
    scene.synchronize()?;
    scene.collect(Vec3::ZERO, 0xf0, &mut query)?;
    assert_eq!(root_guids(&query), [10, 90]);
    scene.world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Replacement",
        Vec3::ZERO,
        0.,
    ));
    assert!(scene.collect(Vec3::ZERO, 0xf0, &mut query).is_err());
    assert!(query.triangles().is_empty());
    scene.synchronize()?;
    scene.collect(Vec3::ZERO, 0xf0, &mut query)?;
    assert!(root_guids(&query).is_empty());
    assert_eq!(
        query.triangles().len(),
        1,
        "retiring replicated roots preserves static global geometry"
    );
    Ok(())
}

#[test]
fn retained_wmo_matrix_updates_match_fresh_placements_and_fail_transactionally()
-> Result<(), Box<dyn Error>> {
    use glam::{Mat4, Quat};
    use solarity_systems::PlacedWorldModelCollision;
    let scene = Scene::new(true)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(scene._fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = Arc::new(solarity_asset::DecodedWorldModel::load(
        &mut store,
        &solarity_asset::AssetPath::new("World\\Wmo\\Fixture.wmo")?,
    )?);
    let mut placed =
        PlacedWorldModelCollision::prepare_transform(Arc::clone(&model), Mat4::IDENTITY)?;
    let bounds = MovementCollisionBounds::new(Vec3::splat(-40.), Vec3::splat(40.))?;
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for cache in [
        MovementBspCacheMode::Enabled,
        MovementBspCacheMode::Disabled,
    ] {
        for transform in [
            Mat4::IDENTITY,
            Mat4::from_scale_rotation_translation(
                Vec3::new(2., 0.5, 1.25),
                Quat::from_rotation_z(0.7),
                Vec3::new(10., -2., 3.),
            ),
            Mat4::from_translation(Vec3::new(-8., 4., -2.)),
        ] {
            placed.set_transform(transform)?;
            let mut fresh =
                PlacedWorldModelCollision::prepare_transform(Arc::clone(&model), transform)?;
            actual.clear();
            expected.clear();
            placed.append_movement(bounds, cache, &mut actual)?;
            fresh.append_movement(bounds, cache, &mut expected)?;
            assert_same_geometry(&actual, &expected);
            assert!(!actual.is_empty());
            for invalid in [Mat4::ZERO, Mat4::from_cols_array(&[f32::NAN; 16])] {
                assert!(placed.set_transform(invalid).is_err());
                actual.clear();
                placed.append_movement(bounds, cache, &mut actual)?;
                assert_same_geometry(&actual, &expected);
            }
            assert!(placed.set_transforms(Mat4::IDENTITY, Mat4::ZERO).is_err());
            actual.clear();
            placed.append_movement(bounds, cache, &mut actual)?;
            assert_same_geometry(&actual, &expected);
        }
    }
    Ok(())
}

#[test]
fn replicated_wmo_roots_collect_doodads_and_register_nearby_props_after_motion()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new(true)?;
    scene.add(90, 43, 11, Vec3::ZERO)?;
    scene.add(10, 42, 5, Vec3::new(-0.75, -0.75, 0.5))?;
    assert_eq!(scene.synchronize()?, RuntimeStaticMovementResidency::Ready);
    let identity = scene.world.object_identity(90).ok_or("identity")?;
    let mut query = RuntimeMovementQuery::new();
    scene.collect(Vec3::ZERO, 0xf001ff, &mut query)?;
    let owners: Vec<_> = (0..query.triangles().len())
        .filter_map(|index| query.owner(index))
        .collect();
    let root = owners
        .iter()
        .position(|owner| *owner == RuntimeMovementOwner::GameObjectWorldModel { identity })
        .ok_or("missing replicated WMO face")?;
    assert_eq!(
        owners[root + 1],
        RuntimeMovementOwner::GameObjectWorldModelDoodad {
            identity,
            doodad_index: 0
        }
    );
    assert!(
        matches!(owners[root + 2], RuntimeMovementOwner::GameObject { identity, .. } if identity.guid() == 10)
    );
    assert_eq!(
        owners
            .iter()
            .filter(|owner| **owner
                == RuntimeMovementOwner::GameObjectWorldModelDoodad {
                    identity,
                    doodad_index: 0
                })
            .count(),
        1
    );
    assert_eq!(generic_guids(&query), [10]);
    assert!(matches!(
        owners[0],
        RuntimeMovementOwner::Static(RuntimeStaticMovementOwner::WorldModel { unique_id: 7 })
    ));
    // Registration must select the replicated transformed-root bank.
    let mut store = scene.objects.object_placement(10).ok_or("prop placement")?;
    let model = Arc::clone(&scene.model);
    let mut placed = PlacedM2Collision::prepare_transform(model, store.matrix())?;
    let mut registration = RuntimeMovementRegistrationQuery::new();
    scene.terrain.register_game_object_movement(
        571,
        &placed,
        MovementBspCacheMode::Enabled,
        &mut registration,
    )?;
    assert_eq!(
        registration
            .selection()
            .and_then(|selection| selection.selected())
            .ok_or("floor")?
            .owner(),
        RuntimeWorldModelMovementOwner::GameObject { identity }
    );
    assert_eq!(
        registration.references(),
        &[RuntimeMovementReference::GameObjectWorldModel { identity, group: 0 }]
    );
    let before: Vec<_> = query.triangles().iter().copied().skip(root).collect();
    scene
        .world
        .update_transform(90, WorldTransform::new(Vec3::X * 10., 0.))?;
    scene
        .world
        .update_transform(10, WorldTransform::new(Vec3::new(9.25, -0.75, 0.5), 0.))?;
    scene.synchronize()?;
    scene.collect(Vec3::X * 10., 0xf001ff, &mut query)?;
    assert_eq!(query.triangles().len(), before.len());
    for (old, current) in before.iter().zip(query.triangles()) {
        for (old, current) in old.vertices().iter().zip(current.vertices()) {
            assert!((*old + Vec3::X * 10. - current).length() < 0.0001);
        }
    }
    store = scene.objects.object_placement(10).ok_or("moved prop")?;
    placed.set_transform(store.matrix())?;
    scene.terrain.register_game_object_movement(
        571,
        &placed,
        MovementBspCacheMode::Enabled,
        &mut registration,
    )?;
    assert_eq!(
        registration.references(),
        &[RuntimeMovementReference::GameObjectWorldModel { identity, group: 0 }]
    );
    scene.world.remove_object(90)?;
    scene.synchronize()?;
    scene.collect(Vec3::X * 10., 0xf001ff, &mut query)?;
    assert!(
        query.triangles().is_empty(),
        "global map has no terrain fallback for the unregistered prop"
    );
    Ok(())
}
