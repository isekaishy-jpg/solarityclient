//! Real terrain references for admitted transport CM2MapObjects.

use std::error::Error;
use std::io::Cursor;

use glam::Vec3;
use solarity_asset::MapCatalog;
use solarity_ecs::{ActiveWorld, GameObjectMovement, ObjectKind, WorldTransform};
use solarity_systems::{MovementBspCacheMode, MovementCollisionBounds, project_object_fields};
use wow_adt::AdtVersion;
use wow_adt::builder::AdtBuilder;
use wow_wdt::chunks::MwmoChunk;
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use super::{GameObjectTemplateCache, RuntimeGameObjectPresentation, models, template};
use crate::application::terrain_coordinator::{
    RuntimeMovementOwner, RuntimeMovementQuery, RuntimeStaticMovementResidency,
    RuntimeTerrainCoordinator,
};
use crate::random::CrtRand;
use crate::test_support::bootstrap_texture_blp;

/// A deck triangle with genuine collision arrays, independent of visible vertices.
pub(super) fn model() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = models::model_with_animations(&[0, 162, 163, 164])?;
    for (offset, count, array) in [
        (
            0xd8,
            3_u32,
            [0_u16, 1, 2]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
        (
            0xe0,
            3,
            [-1_f32, -1., 0., 1., -1., 0., 0., 1., 0.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
        ),
        (
            0xe8,
            1,
            [0_f32, 0., 1.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
        ),
    ] {
        let address = bytes.len() as u32;
        bytes[offset..offset + 4].copy_from_slice(&count.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&address.to_le_bytes());
        bytes.extend(array);
    }
    for (index, value) in [-1_f32, -1., -0.1, 1., 1., 0.1, 2.].into_iter().enumerate() {
        let offset = 0xbc + index * 4;
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

/// One resident ADT surrounds the station without loading unrelated map data.
struct Scene {
    world: ActiveWorld,
    objects: RuntimeGameObjectPresentation,
    terrain: RuntimeTerrainCoordinator,
    cache: GameObjectTemplateCache,
    random: CrtRand,
}

impl Scene {
    fn new() -> Result<Self, Box<dyn Error>> {
        Self::with_files(Vec::new())
    }

    /// The same resident map exercises both map-handle resource families.
    fn with_files(extra: MapFiles) -> Result<Self, Box<dyn Error>> {
        let mut files = map_files()?;
        files.extend(extra);
        let sources: Vec<_> = files
            .iter()
            .map(|(path, bytes)| (*path, bytes.as_slice()))
            .collect();
        let mut objects =
            super::presentation_for_route_with_files(&super::route_nodes(&[2]), &sources)?;
        let maps = MapCatalog::load(&mut objects.assets.borrow_mut())?;
        let mut terrain = RuntimeTerrainCoordinator::new(objects.assets.clone(), maps);
        let mut world = super::world(8724, 0)?;
        world.update_transform(7, WorldTransform::new(Vec3::new(20., 5., 10.), 0.))?;
        world.update_transform(9, WorldTransform::new(Vec3::new(20., 5., 0.), 0.))?;
        terrain.synchronize(Some(&world))?;
        objects.synchronize(Some(&world))?;
        let mut cache = GameObjectTemplateCache::new();
        objects.synchronize_templates(&mut cache);
        Ok(Self {
            world,
            objects,
            terrain,
            cache,
            random: CrtRand::new(),
        })
    }

    fn synchronize(&mut self, time_ms: u32) -> Result<(), Box<dyn Error>> {
        self.objects
            .advance_transports(Some(&mut self.world), time_ms)?;
        self.objects
            .synchronize_animations(Some(&self.world), &mut self.random)?;
        assert_eq!(
            self.terrain.synchronize_game_object_movement(
                Some(&self.world),
                &self.objects,
                MovementBspCacheMode::Enabled,
            )?,
            RuntimeStaticMovementResidency::Ready
        );
        Ok(())
    }

    fn collect(
        &mut self,
        center: Vec3,
        flags: u32,
    ) -> Result<RuntimeMovementQuery, Box<dyn Error>> {
        let mut query = RuntimeMovementQuery::new();
        assert_eq!(
            self.terrain.collect_movement(
                &self.world,
                &self.objects,
                MovementCollisionBounds::new(center - Vec3::splat(3.), center + Vec3::splat(3.))?,
                flags,
                MovementBspCacheMode::Enabled,
                &mut query,
            )?,
            RuntimeStaticMovementResidency::Ready
        );
        Ok(query)
    }
}

#[test]
fn transport_collision_requires_template_uses_upper_mask_and_moves_with_route()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new()?;
    let station = Vec3::new(20., 5., 0.);
    scene.synchronize(3462)?;
    assert!(scene.collect(station, 0xf00000)?.triangles().is_empty());
    scene.cache.receive(template(0)?);
    scene.synchronize(3462)?;
    let query = scene.collect(station, 0xf00000)?;
    assert_eq!(query.triangles().len(), 1);
    let identity = scene.world.object_identity(9).ok_or("transport identity")?;
    assert_eq!(
        query.owner(0),
        Some(RuntimeMovementOwner::GameObjectMapModel { identity })
    );
    assert!(scene.collect(station, 0xf)?.triangles().is_empty());
    let initial_vertices = query.triangles()[0].vertices();
    let instance = scene
        .objects
        .movement_instance(identity)
        .ok_or("instance")?;
    assert!(instance.behavior().is_none());
    let collision = instance
        .transport_model()
        .ok_or("map model")?
        .collision()
        .ok_or("collision")?;
    assert_eq!(
        collision.transform(),
        scene
            .objects
            .object_placement(9)
            .ok_or("placement")?
            .matrix()
    );
    drop(collision);

    scene
        .objects
        .advance_transports(Some(&mut scene.world), 6100)?;
    scene
        .objects
        .synchronize_animations(Some(&scene.world), &mut scene.random)?;
    // A query cannot silently consume a model moved since its references were published.
    assert!(scene.collect(station, 0xf00000).is_err());
    scene.synchronize(6100)?;
    let pose = scene
        .objects
        .object_placement(9)
        .ok_or("moved placement")?
        .matrix();
    let moved = scene.collect(pose.w_axis.truncate(), 0xf00000)?;
    assert_eq!(moved.triangles().len(), 1);
    assert_ne!(moved.triangles()[0].vertices(), initial_vertices);
    for (actual, local) in moved.triangles()[0].vertices().iter().zip([
        Vec3::new(-1., -1., 0.),
        Vec3::new(1., -1., 0.),
        Vec3::new(0., 1., 0.),
    ]) {
        assert!(actual.abs_diff_eq(pose.transform_point3(local), 0.00001));
    }
    scene.world.remove_object(9)?;
    assert!(
        scene
            .collect(pose.w_axis.truncate(), 0xf00000)?
            .triangles()
            .is_empty()
    );
    scene.world.create_object(
        9,
        ObjectKind::GameObject,
        Some(WorldTransform::new(station, 0.)),
        [],
    )?;
    scene
        .world
        .update_game_object_movement(9, GameObjectMovement::default())?;
    project_object_fields(
        &mut scene.world,
        9,
        [(4, 1_f32.to_bits()), (8, 42), (17, 15 << 8)],
    )?;
    assert!(
        scene
            .collect(pose.w_axis.truncate(), 0xf00000)?
            .triangles()
            .is_empty()
    );
    Ok(())
}

#[test]
fn empty_route_keeps_initial_scale_one_map_handle_and_collision() -> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new()?;
    super::fields(&mut scene.world, &[(4, 3_f32.to_bits()), (16, 0)])?;
    scene.cache.receive(template(0)?);
    scene.synchronize(100)?;
    assert!(scene.world.game_object_animated_pose(9).is_none());
    let placement = scene
        .objects
        .object_placement(9)
        .ok_or("initial map placement")?;
    assert_eq!(placement.matrix().x_axis.truncate().length(), 1.);
    let identity = scene.world.object_identity(9).ok_or("transport identity")?;
    let passenger = scene
        .objects
        .object_movement_frame(identity)?
        .ok_or("passenger frame")?;
    assert_eq!(passenger.world_matrix().x_axis.truncate().length(), 3.);
    assert_eq!(passenger.world_matrix().w_axis, placement.matrix().w_axis);
    assert_eq!(
        scene
            .collect(Vec3::new(20., 5., 0.), 0xf00000)?
            .triangles()
            .len(),
        1
    );
    scene.synchronize(6100)?;
    assert_eq!(scene.objects.object_placement(9), Some(placement));
    assert_eq!(
        scene
            .objects
            .object_movement_frame(identity)?
            .ok_or("retained frame")?
            .world_matrix(),
        passenger.world_matrix()
    );
    scene.world.remove_object(9)?;
    scene.objects.synchronize(Some(&scene.world))?;
    assert!(scene.objects.object_movement_frame(identity)?.is_none());
    Ok(())
}

#[test]
fn transport_map_models_precede_generic_callbacks_in_retained_object_order()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new()?;
    let station = Vec3::new(20., 5., 0.);
    for (guid, kind) in [(11, 5_u32), (12, 15)] {
        scene.world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(station, 0.)),
            [],
        )?;
        scene.world.update_game_object_movement(
            guid,
            GameObjectMovement::default().with_transport_clock(0, 100),
        )?;
        project_object_fields(
            &mut scene.world,
            guid,
            [
                (3, 42),
                (4, 1_f32.to_bits()),
                (8, 42),
                (14, 0xffff0000),
                (16, 8724),
                (17, (kind << 8) | 1),
            ],
        )?;
    }
    scene.objects.synchronize(Some(&scene.world))?;
    scene.objects.synchronize_templates(&mut scene.cache);
    scene.cache.receive(template(0)?);
    for _ in 0..2 {
        scene.synchronize(3462)?;
        let query = scene.collect(station, 0xf00000)?;
        let map_owner = |guid| {
            scene
                .world
                .object_identity(guid)
                .map(|identity| RuntimeMovementOwner::GameObjectMapModel { identity })
        };
        assert_eq!(query.triangles().len(), 3);
        assert_eq!(query.owner(0), map_owner(9));
        assert_eq!(query.owner(1), map_owner(12));
        assert_eq!(
            query.owner(2),
            Some(RuntimeMovementOwner::GameObject {
                identity: scene.world.object_identity(11).ok_or("generic identity")?,
                reported_guid: 11,
            })
        );
    }
    Ok(())
}

#[test]
fn empty_route_retains_its_reference_slot_while_sampled_station_moves_to_tail()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new()?;
    let station = Vec3::new(20., 5., 0.);
    scene.world.create_object(
        12,
        ObjectKind::GameObject,
        Some(WorldTransform::new(station, 0.)),
        [],
    )?;
    scene
        .world
        .update_game_object_movement(12, GameObjectMovement::default())?;
    project_object_fields(
        &mut scene.world,
        12,
        [
            (3, 42),
            (4, 1_f32.to_bits()),
            (8, 42),
            (16, 0),
            (17, 15 << 8),
        ],
    )?;
    scene.objects.synchronize(Some(&scene.world))?;
    scene.objects.synchronize_templates(&mut scene.cache);
    scene.cache.receive(template(0)?);
    for expected in [[9, 12], [12, 9]] {
        scene.synchronize(3462)?;
        let query = scene.collect(station, 0xf00000)?;
        assert_eq!(query.triangles().len(), 2);
        for (index, guid) in expected.into_iter().enumerate() {
            assert_eq!(
                query.owner(index),
                Some(RuntimeMovementOwner::GameObjectMapModel {
                    identity: scene.world.object_identity(guid).ok_or("identity")?,
                })
            );
        }
    }
    Ok(())
}

#[test]
fn map_handle_retention_and_boarding_have_independent_lifetime_gates() -> Result<(), Box<dyn Error>>
{
    let mut scene = Scene::new()?;
    let identity = scene.world.object_identity(9).ok_or("identity")?;
    assert!(!scene.objects.object_can_board(identity));
    assert!(
        scene
            .objects
            .object_retains_passenger(identity, Vec3::splat(100.))?
    );
    super::fields(&mut scene.world, &[(9, 8)])?;
    scene.objects.synchronize(Some(&scene.world))?;
    assert!(scene.objects.object_can_board(identity));
    scene.cache.receive(template(0)?);
    scene.synchronize(3462)?;
    // The model's authored box tops at 0.1; native headroom extends above it.
    for (position, expected) in [
        (Vec3::ZERO, true),
        (Vec3::new(0., 0., 1.7), true),
        (Vec3::new(0., 0., 1.8), false),
        (Vec3::new(1.01, 0., 0.), false),
        (Vec3::new(0., 0., -0.11), false),
    ] {
        assert_eq!(
            scene.objects.object_retains_passenger(identity, position)?,
            expected
        );
    }
    // Removing the boarding flag does not change an existing passenger's volume.
    super::fields(&mut scene.world, &[(9, 0)])?;
    scene.objects.synchronize(Some(&scene.world))?;
    assert!(!scene.objects.object_can_board(identity));
    assert!(
        scene
            .objects
            .object_retains_passenger(identity, Vec3::ZERO)?
    );
    scene.world.remove_object(9)?;
    scene.objects.synchronize(Some(&scene.world))?;
    assert!(!scene.objects.object_can_board(identity));
    assert!(
        !scene
            .objects
            .object_retains_passenger(identity, Vec3::ZERO)?
    );
    Ok(())
}

#[test]
fn wmo_transport_waits_for_map_handle_then_uses_authored_retention_planes()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::with_files(wmo_files())?;
    let identity = scene.world.object_identity(9).ok_or("identity")?;
    let station = Vec3::new(20., 5., 0.);
    scene.synchronize(3462)?;
    assert!(scene.collect(station, 0xf001ff)?.triangles().is_empty());
    assert!(
        scene
            .objects
            .object_retains_passenger(identity, Vec3::splat(100.))?
    );
    scene.cache.receive(template(0)?);
    scene.synchronize(3462)?;
    let query = scene.collect(station, 0xf001ff)?;
    assert_eq!(query.triangles().len(), 1);
    assert_eq!(
        query.owner(0),
        Some(RuntimeMovementOwner::GameObjectWorldModel { identity })
    );
    // MCVP is deliberately narrower than the MOGI/MOGP/root collision boxes.
    assert!(
        scene
            .objects
            .object_retains_passenger(identity, Vec3::new(0.5, 0., 0.))?
    );
    assert!(
        !scene
            .objects
            .object_retains_passenger(identity, Vec3::new(0.51, 0., 0.))?
    );
    scene.world.remove_object(9)?;
    scene.objects.synchronize(Some(&scene.world))?;
    scene.synchronize(3462)?;
    assert!(scene.collect(station, 0xf001ff)?.triangles().is_empty());
    assert!(
        !scene
            .objects
            .object_retains_passenger(identity, Vec3::ZERO)?
    );
    Ok(())
}

/// One collision-only root with an authored X <= 0.5 retention plane.
fn wmo_files() -> MapFiles {
    let mut root = Vec::new();
    chunk(&mut root, b"REVM", &17_u32.to_le_bytes());
    let bounds = [-1_f32, -1., -0.1, 1., 1., 0.1];
    let bounds_bytes: Vec<_> = bounds.into_iter().flat_map(f32::to_le_bytes).collect();
    let mut header = [0_u8; 64];
    header[4..8].copy_from_slice(&1_u32.to_le_bytes());
    header[36..60].copy_from_slice(&bounds_bytes);
    chunk(&mut root, b"DHOM", &header);
    let mut info = [0_u8; 32];
    info[4..28].copy_from_slice(&bounds_bytes);
    info[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    chunk(&mut root, b"IGOM", &info);
    chunk(
        &mut root,
        b"PVCM",
        &[1_f32, 0., 0., -0.5]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let mut body = vec![0_u8; 68];
    body[12..36].copy_from_slice(&bounds_bytes);
    chunk(&mut body, b"YPOM", &[8, 255]);
    chunk(&mut body, b"IVOM", &[0, 0, 1, 0, 2, 0]);
    chunk(
        &mut body,
        b"TVOM",
        &[-1_f32, -1., 0., 1., -1., 0., 0., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    chunk(
        &mut body,
        b"RNOM",
        &[0_f32, 0., 1., 0., 0., 1., 0., 0., 1.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let mut node = vec![4, 0, 255, 255, 255, 255, 1, 0];
    node.extend([0_u8; 8]);
    chunk(&mut body, b"NBOM", &node);
    chunk(&mut body, b"RBOM", &[0, 0]);
    let mut group = Vec::new();
    chunk(&mut group, b"REVM", &17_u32.to_le_bytes());
    chunk(&mut group, b"PGOM", &body);
    let mut row = vec![0_u32; 19];
    row[0] = 42;
    row[1] = 1;
    row[12..18].copy_from_slice(&bounds.map(f32::to_bits));
    let mut displays = super::table(19, &[row]);
    displays.pop();
    let path = b"\0World\\Transport.wmo\0";
    displays.extend_from_slice(path);
    displays[16..20].copy_from_slice(&(path.len() as u32).to_le_bytes());
    vec![
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", displays),
        ("World\\Transport.wmo", root),
        ("World\\Transport_000.wmo", group),
    ]
}

/// Preserve raw WMO chunk lengths, including nested MOGP subchunks.
fn chunk(bytes: &mut Vec<u8>, magic: &[u8; 4], payload: &[u8]) {
    bytes.extend(magic);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
}

/// WDBC/WDT/ADT fixtures use exact map zero and the native station's tile (31,31).
fn map_files() -> Result<MapFiles, Box<dyn Error>> {
    let mut fields = vec![0_u32; 66];
    fields[1] = 1;
    fields[5] = 1;
    fields[59] = u32::MAX;
    fields[63] = 2;
    let mut map = super::table(66, &[fields]);
    map.pop();
    map.extend_from_slice(b"\0Fixture\0");
    map[16..20].copy_from_slice(&9_u32.to_le_bytes());
    let mut manifest = WdtFile::new(WowVersion::WotLK);
    manifest.mwmo = Some(MwmoChunk::new());
    manifest
        .main
        .get_mut(31, 31)
        .ok_or("tile")?
        .set_has_adt(true);
    let mut wdt = Vec::new();
    WdtWriter::new(&mut wdt).write(&manifest)?;
    let adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut Cursor::new(adt))? else {
        return Err("not a root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        chunk.header.position = [
            17_066.666_f32 - (31 * 16 + chunk.header.index_y) as f32 * 33.333_332,
            17_066.666_f32 - (31 * 16 + chunk.header.index_x) as f32 * 33.333_332,
            -10.,
        ];
        chunk
            .heights
            .as_mut()
            .ok_or("missing heights")?
            .heights
            .fill(0.);
    }
    Ok(vec![
        ("DBFilesClient\\Map.dbc", map),
        ("World\\Maps\\Fixture\\Fixture.wdt", wdt),
        (
            "World\\Maps\\Fixture\\Fixture_31_31.adt",
            wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?,
        ),
        ("tileset\\fixture\\grass.blp", bootstrap_texture_blp()),
    ])
}

/// Owned archive entries retained while the fixture mounts its synthetic map.
type MapFiles = Vec<(&'static str, Vec<u8>)>;
