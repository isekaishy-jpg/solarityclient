//! External integration tests for loading-card transport resource residency.

use std::error::Error;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, GameObjectDisplayCatalog, Locale,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{
    ActiveWorld, GameObjectMovement, GameObjectPresentation, GameObjectTransport, ObjectKind,
    ObjectPresentation, WorldBootstrap, WorldMapId, WorldMovementContext, WorldMovementSpeeds,
    WorldMovementState, WorldMovementTransport, WorldTransform,
};
use solarity_runtime::{
    RuntimeTransportPoll, RuntimeTransportPresentation, RuntimeTransportResourceKind,
};

use crate::support::ClientFixture;

/// Stock `0x00409800` waits for the named object and its concrete WMO request.
#[test]
fn referenced_transport_admits_the_exact_world_model_generation() -> Result<(), Box<dyn Error>> {
    let displays = game_object_display_table();
    let root_wmo = root_wmo_fixture();
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", &displays),
        ("World\\Wmo\\Transport\\Fixture.wmo", &root_wmo),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let archive_catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive_catalog.clone())?;
    let catalog = GameObjectDisplayCatalog::load(&mut store)?;
    let mut presentation = RuntimeTransportPresentation::new(AssetStoreHandle::new(store), catalog)
        .with_worker_catalog(archive_catalog);
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;

    let player_guid = 0x0000_0000_0000_0042;
    let transport_guid = 0xF110_0000_0000_002A;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        player_guid,
        "TransportFixture",
        Vec3::ZERO,
        0.0,
    ));
    world.update_movement(
        player_guid,
        WorldMovementState::new(
            0x200,
            WorldMovementSpeeds::new([0.0; 9]),
            WorldMovementContext {
                transport: Some(WorldMovementTransport {
                    guid: transport_guid,
                    position: Vec3::ZERO,
                    orientation: 0.0,
                    time_ms: 0,
                    seat: -1,
                    interpolated_time_ms: None,
                }),
                ..WorldMovementContext::default()
            },
        ),
    )?;

    assert_eq!(
        presentation.synchronize_async(Some(&world), &cpu)?,
        RuntimeTransportPoll::AwaitingObject {
            guid: transport_guid
        }
    );
    assert!(!presentation.is_ready());

    let entity = world.create_object(
        transport_guid,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::new(10.0, 20.0, 30.0), 0.5)),
        [],
    )?;
    world.storage_mut().add_component(
        entity,
        (
            ObjectPresentation::new(7, 1.25),
            GameObjectPresentation::new(42, 1),
        ),
    );

    assert_eq!(
        presentation.synchronize_async(Some(&world), &cpu)?,
        RuntimeTransportPoll::Pending {
            guid: transport_guid,
            kind: RuntimeTransportResourceKind::WorldModel,
        }
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let poll = presentation.synchronize_async(Some(&world), &cpu)?;
        if poll
            == (RuntimeTransportPoll::ResourceLoaded {
                guid: transport_guid,
                kind: RuntimeTransportResourceKind::WorldModel,
            })
        {
            break;
        }
        assert!(Instant::now() < deadline, "transport worker did not finish");
        std::thread::yield_now();
    }
    assert!(presentation.is_ready());
    assert_eq!(presentation.resident_guid(), Some(transport_guid));
    assert_eq!(presentation.resident_display_id(), Some(42));
    assert_eq!(presentation.resident_state(), Some(1));
    assert_eq!(presentation.resident_scale(), Some(1.25));
    assert_eq!(
        presentation
            .resident_transform()
            .ok_or("transport transform was not retained")?
            .position(),
        Vec3::new(10.0, 20.0, 30.0)
    );
    assert_eq!(
        presentation
            .resident_asset_path()
            .ok_or("transport WMO path was not retained")?
            .as_str(),
        "WORLD\\WMO\\TRANSPORT\\FIXTURE.WMO"
    );
    assert_eq!(
        presentation.synchronize_async(Some(&world), &cpu)?,
        RuntimeTransportPoll::Current {
            guid: transport_guid,
            kind: RuntimeTransportResourceKind::WorldModel,
        }
    );

    world.storage_mut().add_component(
        entity,
        (
            ObjectPresentation::new(7, 1.25),
            GameObjectPresentation::new(42, 0),
        ),
    );
    assert_eq!(
        presentation.synchronize_async(Some(&world), &cpu)?,
        RuntimeTransportPoll::Current {
            guid: transport_guid,
            kind: RuntimeTransportResourceKind::WorldModel,
        }
    );
    assert_eq!(presentation.resident_state(), Some(0));

    let packed_rotation = 0x4000_0000_0000_0000;
    world.update_game_object_movement(
        transport_guid,
        GameObjectMovement::new(packed_rotation, None),
    )?;
    assert_eq!(
        presentation.synchronize_async(Some(&world), &cpu)?,
        RuntimeTransportPoll::Current {
            guid: transport_guid,
            kind: RuntimeTransportResourceKind::WorldModel,
        }
    );
    let rotated = presentation
        .resident_placement()
        .ok_or("missing full transport placement")?;
    assert_eq!(
        rotated.rotation(),
        solarity_systems::unpack_game_object_rotation(packed_rotation)
    );
    assert!(rotated.matrix().y_axis.z > 1.0);
    assert!(presentation.resident_placement_error().is_none());

    let parent_guid = 0xF110_0000_0000_003B;
    world.update_game_object_movement(
        transport_guid,
        GameObjectMovement::new(
            packed_rotation,
            Some(GameObjectTransport {
                guid: parent_guid,
                position: Vec3::new(2.0, 3.0, 4.0),
                orientation: 0.0,
            }),
        ),
    )?;
    let changed = RuntimeTransportPoll::PlacementChanged {
        guid: transport_guid,
        kind: RuntimeTransportResourceKind::WorldModel,
    };
    assert_eq!(presentation.synchronize_async(Some(&world), &cpu)?, changed);
    assert!(presentation.resident_placement().is_none());
    assert_eq!(
        presentation.resident_placement_error(),
        Some(solarity_systems::GameObjectPlacementError::MissingObject { guid: parent_guid })
    );
    assert!(
        presentation.is_ready(),
        "resource readiness must remain independent of placement"
    );
    assert_eq!(presentation.resident_guid(), Some(transport_guid));

    let parent_entity = world.create_object(
        parent_guid,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::splat(10.0), 0.0)),
        [],
    )?;
    world
        .storage_mut()
        .add_component(parent_entity, (ObjectPresentation::new(1, 2.0),));
    assert_eq!(presentation.synchronize_async(Some(&world), &cpu)?, changed);
    assert_eq!(
        presentation
            .resident_placement()
            .ok_or("parent arrival did not restore placement")?
            .matrix()
            .w_axis
            .truncate(),
        Vec3::new(14.0, 16.0, 18.0)
    );
    world.remove_object(parent_guid)?;
    assert_eq!(presentation.synchronize(Some(&world))?, changed);
    assert!(presentation.resident_placement().is_none());
    world.update_game_object_movement(transport_guid, GameObjectMovement::default())?;
    assert_eq!(presentation.synchronize(Some(&world))?, changed);
    assert_eq!(
        presentation
            .resident_placement()
            .ok_or("detach did not restore placement")?
            .matrix()
            .w_axis
            .truncate(),
        Vec3::new(10.0, 20.0, 30.0)
    );

    world.remove_object(transport_guid)?;
    assert_eq!(
        presentation.synchronize_async(Some(&world), &cpu)?,
        RuntimeTransportPoll::AwaitingObject {
            guid: transport_guid
        }
    );
    assert!(!presentation.is_ready());
    assert_eq!(presentation.resident_guid(), None);
    cpu.shutdown()?;
    Ok(())
}

/// Builds the exact 19-word display row used by the dynamic owner.
fn game_object_display_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let path = append_string(&mut strings, "World\\Wmo\\Transport\\Fixture.wmo");
    let mut fields = [0_u32; 19];
    fields[0] = 42;
    fields[1] = path;
    for (slot, value) in fields[12..18]
        .iter_mut()
        .zip([-2.0_f32, -3.0, -4.0, 2.0, 3.0, 4.0])
    {
        *slot = value.to_bits();
    }
    wdbc_fixture(&fields, &strings)
}

/// Builds a valid zero-group WMO root without unrelated embedded doodads.
fn root_wmo_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 32, 42);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    set_u16(&mut header, 60, 0x8);
    push_chunk(&mut bytes, *b"DHOM", &header);
    push_chunk(&mut bytes, *b"IGOM", &[]);
    bytes
}

/// Encodes one ordinary WDBC generation.
fn wdbc_fixture(fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&((fields.len() * 4) as u32).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Appends one NUL-terminated DBC string and returns its byte offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

/// Appends one reversed-magic WMO chunk.
fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

/// Writes one little-endian header word.
fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Writes one little-endian header halfword.
fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

/// Writes one packed three-float WMO vector.
fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        bytes[offset + axis * 4..offset + axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
}
