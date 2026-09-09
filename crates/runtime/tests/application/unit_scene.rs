//! Decoded WMO registration and camera traversal drive indoor unit collision.

#[path = "unit_scene_overlap.rs"]
mod overlap;

use std::{error::Error, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::{
    MovementBspCacheMode, WorldModelRegistrationKind, WorldModelRegistrationQuery,
};

use super::{
    MovementCollisionBounds, PlacedWorldModelCollision, RuntimeWorldModelMovementOwner,
    UnitSceneAdmission, WorldModelCameraRegistration, WorldSceneCameraFrame, WorldSceneDepthFrame,
};
use crate::test_support::ClientFixture;

/// Either primary bank may reach 793270, even outside the model view window.
#[test]
fn indoor_unit_uses_both_primary_roots_and_exact_owner_identity() -> Result<(), Box<dyn Error>> {
    let mut root = floor_root(0, 0)?;
    let first = owner(17);
    let second = owner(29);
    let mut query = unit_query()?;
    query.probe_root(
        first,
        WorldModelRegistrationKind::Static,
        &mut root,
        MovementBspCacheMode::Enabled,
    )?;
    query.probe_root(
        second,
        WorldModelRegistrationKind::Transformed,
        &mut root,
        MovementBspCacheMode::Enabled,
    )?;
    let selection = query.finish();
    assert_eq!(
        selection
            .selected()
            .ok_or("fixture floor registration missing")?
            .owner(),
        second
    );
    assert!(
        selection
            .selected()
            .ok_or("fixture floor registration missing")?
            .hit()
            .is_interior()
    );
    assert_eq!(selection.primary().into_iter().flatten().count(), 2);
    let bounds = MovementCollisionBounds::new(Vec3::splat(10000.), Vec3::splat(10001.))?;
    for admitted in [first, second, owner(90)] {
        let mut scene = UnitSceneAdmission::default();
        scene.record_camera_root(&root, camera()?, registration(admitted), first)?;
        assert_eq!(
            scene.admits_registration(selection, bounds)?,
            admitted != owner(90)
        );
    }
    Ok(())
}

/// 79A260 masks MOGI exterior groups only when they belong to the secondary root.
#[test]
fn secondary_camera_root_excludes_exterior_group_unit_callbacks() -> Result<(), Box<dyn Error>> {
    for flags in [0, 8] {
        // Different MOGI/MOGP flags expose the scene gate independently of the
        // loaded group's interior unit classification, as the native code does.
        let mut root = floor_root(flags, 0)?;
        let selected = owner(29);
        let mut query = unit_query()?;
        query.probe_root(
            selected,
            WorldModelRegistrationKind::Static,
            &mut root,
            MovementBspCacheMode::Enabled,
        )?;
        let selection = query.finish();
        assert!(
            selection
                .selected()
                .ok_or("fixture floor registration missing")?
                .hit()
                .is_interior()
        );
        for primary in [selected, owner(17)] {
            let mut scene = UnitSceneAdmission::default();
            scene.record_camera_root(&root, camera()?, registration(selected), primary)?;
            assert_eq!(
                scene.admits_registration(selection, unit_bounds()?)?,
                primary == selected || flags == 0
            );
        }
    }
    Ok(())
}

/// Exterior-registered units remain depth-listed even if their WMO group is visited.
#[test]
fn exterior_unit_requires_outdoor_depth_admission() -> Result<(), Box<dyn Error>> {
    let mut root = floor_root(8, 8)?;
    let selected = owner(17);
    let mut query = unit_query()?;
    query.probe_root(
        selected,
        WorldModelRegistrationKind::Static,
        &mut root,
        MovementBspCacheMode::Enabled,
    )?;
    let selection = query.finish();
    assert!(
        !selection
            .selected()
            .ok_or("fixture floor registration missing")?
            .hit()
            .is_interior()
    );
    let mut scene = UnitSceneAdmission::default();
    scene.record_camera_root(&root, camera()?, registration(selected), selected)?;
    assert!(!scene.admits_registration(selection, unit_bounds()?)?);
    scene.outdoor = Some(WorldSceneDepthFrame::new(
        Vec3::new(-1., -1., 1.),
        Vec3::new(0., -1., 1.),
    )?);
    assert!(scene.admits_registration(selection, unit_bounds()?)?);
    assert!(!scene.admits_registration(
        selection,
        MovementCollisionBounds::new(Vec3::new(100000., 0., 0.), Vec3::new(100001., 1., 1.))?
    )?);
    Ok(())
}

/// An outdoor camera reaches an interior unit only through a visible entrance.
#[test]
fn outdoor_building_portal_admits_registered_indoor_unit() -> Result<(), Box<dyn Error>> {
    let mut root = scene_root(0, 0, true)?;
    let selected = owner(17);
    let mut query = unit_query()?;
    query.probe_root(
        selected,
        WorldModelRegistrationKind::Static,
        &mut root,
        MovementBspCacheMode::Enabled,
    )?;
    let selection = query.finish();
    let hit = selection
        .selected()
        .ok_or("fixture floor registration missing")?
        .hit();
    assert_eq!(hit.group_index(), 1);
    assert!(hit.is_interior());
    let eye = Vec3::new(-30., -1., 1.);
    for (direction, window, admitted) in [
        (Vec3::X, [0., 0., 1., 1.], true),
        (-Vec3::X, [0., 0., 1., 1.], false),
        (Vec3::X, [0., 0., 0.1, 0.1], false),
    ] {
        let camera = WorldSceneCameraFrame::perspective(
            eye,
            eye + direction,
            direction,
            Vec3::Z,
            1.,
            1.5,
            [0.1, 1000.],
        )?;
        let mut scene = UnitSceneAdmission::default();
        let _ = scene.record_outdoor_root(
            &root,
            camera,
            selected,
            None,
            WorldSceneDepthFrame::new(eye, eye + direction)?,
            window,
        )?;
        // The interior bit is set before the unit's own draw frustum test.
        let hidden_bounds = MovementCollisionBounds::new(Vec3::splat(10000.), Vec3::splat(10001.))?;
        assert_eq!(
            scene.admits_registration(selection, hidden_bounds)?,
            admitted
        );
        assert!(!scene.groups.contains(&(selected, 0)));
    }
    Ok(())
}

/// 792AD0 excludes ordinary interior groups from outdoor entry lists.
#[test]
fn outdoor_scene_cannot_enter_a_root_without_exterior_groups() -> Result<(), Box<dyn Error>> {
    let root = floor_root(0, 0)?;
    let mut scene = UnitSceneAdmission::default();
    let _ = scene.record_outdoor_root(
        &root,
        camera()?,
        owner(17),
        None,
        WorldSceneDepthFrame::new(Vec3::ZERO, Vec3::X)?,
        [0., 0., 1., 1.],
    )?;
    assert!(scene.groups.is_empty());
    Ok(())
}

/// Keeps root lifetimes visibly distinct in each scene fixture.
fn owner(unique_id: u32) -> RuntimeWorldModelMovementOwner {
    RuntimeWorldModelMovementOwner::Static { unique_id }
}

/// Camera begins in the fixture's only group; no exterior portals are authored.
fn registration(
    owner: RuntimeWorldModelMovementOwner,
) -> WorldModelCameraRegistration<RuntimeWorldModelMovementOwner> {
    WorldModelCameraRegistration {
        owner,
        group: 0,
        secondary_group: None,
    }
}

/// Uses a real perspective frame with the retained native direction.
fn camera() -> Result<WorldSceneCameraFrame, Box<dyn Error>> {
    let eye = Vec3::new(-1., -1., 1.);
    Ok(WorldSceneCameraFrame::perspective(
        eye,
        eye + Vec3::X,
        Vec3::X,
        Vec3::Z,
        1.,
        1.5,
        [0.1, 1000.],
    )?)
}

/// Unit_C's biased downward registration ray intersects the authored floor.
fn unit_query()
-> Result<WorldModelRegistrationQuery<RuntimeWorldModelMovementOwner>, Box<dyn Error>> {
    let start = Vec3::new(-1., -1., 1.1);
    Ok(WorldModelRegistrationQuery::new(
        start,
        Vec3::new(-1., -1., -999.),
        start,
    )?)
}

/// Raw unit box straddles its registered position.
fn unit_bounds() -> Result<MovementCollisionBounds, Box<dyn Error>> {
    Ok(MovementCollisionBounds::new(
        Vec3::new(-2., -2., 0.),
        Vec3::new(0., 0., 2.),
    )?)
}

/// Builds an actual floor BSP and decodes it through an isolated MPQ profile.
fn floor_root(
    info_flags: u32,
    group_flags: u32,
) -> Result<PlacedWorldModelCollision, Box<dyn Error>> {
    scene_root(info_flags, group_flags, false)
}

/// Optionally adds an exterior group with a portal into the real floor BSP.
fn scene_root(
    info_flags: u32,
    group_flags: u32,
    entrance: bool,
) -> Result<PlacedWorldModelCollision, Box<dyn Error>> {
    let mut root = Vec::new();
    chunk(&mut root, b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0; 64];
    word(&mut header, 4, 1 + u32::from(entrance));
    word(&mut header, 8, u32::from(entrance));
    let bounds = [-16f32, -16., -16., 16., 16., 16.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    header[36..60].copy_from_slice(&bounds);
    chunk(&mut root, b"DHOM", &header);
    let mut info = vec![0; 32];
    word(&mut info, 0, info_flags);
    info[4..28].copy_from_slice(&bounds);
    word(&mut info, 28, u32::MAX);
    if entrance {
        let mut exterior = info.clone();
        word(&mut exterior, 0, 8);
        exterior.extend(info);
        info = exterior;
    }
    chunk(&mut root, b"IGOM", &info);
    if entrance {
        let vertices = [
            [-3f32, -3., -1.],
            [-3., -3., 4.],
            [-3., 3., 4.],
            [-3., 3., -1.],
        ]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
        chunk(&mut root, b"VPOM", &vertices);
        let mut portal = vec![0; 20];
        portal[2..4].copy_from_slice(&4u16.to_le_bytes());
        portal[4..8].copy_from_slice(&(-1f32).to_le_bytes());
        portal[16..20].copy_from_slice(&(-3f32).to_le_bytes());
        chunk(&mut root, b"TPOM", &portal);
        let references = [0u16, 1, 1, 0, 0, 0, u16::MAX, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        chunk(&mut root, b"RPOM", &references);
    }
    let mut header = vec![0; 68];
    word(&mut header, 8, group_flags);
    header[12..36].copy_from_slice(&bounds);
    let mut exterior_group = Vec::new();
    if entrance {
        let mut exterior_header = header.clone();
        word(&mut exterior_header, 8, 8);
        exterior_header[38..40].copy_from_slice(&1u16.to_le_bytes());
        chunk(&mut exterior_group, b"REVM", &17u32.to_le_bytes());
        chunk(&mut exterior_group, b"PGOM", &exterior_header);
        header[36..38].copy_from_slice(&1u16.to_le_bytes());
        header[38..40].copy_from_slice(&1u16.to_le_bytes());
    }
    chunk(&mut header, b"YPOM", &[8, 255]);
    chunk(&mut header, b"IVOM", &[0, 0, 1, 0, 2, 0]);
    let vertices = [[-3f32, -3., 0.], [3., -3., 0.], [-3., 3., 0.]]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, b"TVOM", &vertices);
    let normals = [[0f32, 0., 1.]; 3]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, b"RNOM", &normals);
    let mut node = vec![0; 16];
    node[0..2].copy_from_slice(&4u16.to_le_bytes());
    node[2..6].fill(255);
    node[6..8].copy_from_slice(&1u16.to_le_bytes());
    chunk(&mut header, b"NBOM", &node);
    chunk(&mut header, b"RBOM", &0u16.to_le_bytes());
    let mut group = Vec::new();
    chunk(&mut group, b"REVM", &17u32.to_le_bytes());
    chunk(&mut group, b"PGOM", &header);
    let mut files = vec![("World\\Scene.wmo", root.as_slice())];
    if entrance {
        files.push(("World\\Scene_000.wmo", exterior_group.as_slice()));
        files.push(("World\\Scene_001.wmo", group.as_slice()));
    } else {
        files.push(("World\\Scene_000.wmo", group.as_slice()));
    }
    let fixture = ClientFixture::with_common_files(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Scene.wmo")?,
    )?);
    Ok(PlacedWorldModelCollision::prepare_transform(
        model,
        Mat4::IDENTITY,
    )?)
}

/// Encodes a reversed FourCC chunk with its exact payload length.
fn chunk(output: &mut Vec<u8>, id: &[u8; 4], payload: &[u8]) {
    output.extend(id);
    output.extend((payload.len() as u32).to_le_bytes());
    output.extend(payload);
}

/// Writes a little-endian fixed-layout WMO word.
fn word(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
