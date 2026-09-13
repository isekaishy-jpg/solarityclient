//! True-exterior admission controls WDL publication independently of visible sky.

use std::{error::Error, path::PathBuf, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
    TerrainLowDetail,
};
use solarity_rendering::{TerrainLowDetailMap, WorldCamera, WorldHorizonScale, WorldScreenWindow};
use solarity_systems::{
    PlacedWorldModelCollision, WorldModelCameraRegistrationQuery, WorldModelRegistrationKind,
    WorldSceneCameraFrame,
};

use super::{WorldSceneAdmission, camera, owner, registration, scene_root_with_skybox};

/// 79A870's sky seed cannot open the separate ADF58C true-exterior bank.
#[test]
fn sky_only_camera_root_does_not_publish_horizon() -> Result<(), Box<dyn Error>> {
    let root = scene_root_with_skybox(0x40, 0x40, false, Some("Sky.m2"))?;
    let mut scene = WorldSceneAdmission::default();
    scene.sky.seed(root.model(), [Some(0), None]);
    assert!(
        scene
            .record_camera_root(&root, camera()?, registration(owner(1)), owner(1))?
            .is_none()
    );
    assert!(scene.sky.has_window());
    let map = horizon_map()?;
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 777.).frame(16. / 9.)?;
    assert!(
        scene
            .horizon_frame(&map, camera, Vec3::ONE, WorldHorizonScale::default())?
            .is_none()
    );
    // The same retained map remains eligible when traversal opens exterior.
    scene.outdoor_window = Some(WorldScreenWindow::FULL);
    assert!(
        scene
            .horizon_frame(&map, camera, Vec3::ONE, WorldHorizonScale::default())?
            .is_some()
    );
    Ok(())
}

/// Local archives reproduce the city's sky-only groups without launching a client.
#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn installed_orgrimmar_sky_only_groups_do_not_publish_horizon() -> Result<(), Box<dyn Error>> {
    let data = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(PathBuf::from(data))?,
        Locale::EnUs,
    )?)?;
    let model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World/Wmo/Kalimdor/Ogrimmar/Ogrimmar.wmo")?,
    )?);
    // MODF 165042, also captured by the existing Orgrimmar scene oracle.
    let transform = Mat4::from_cols_array(&[
        -0.82903755,
        -0.55919296,
        0.,
        0.,
        0.55919296,
        -0.82903755,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        1500.502,
        -4418.1934,
        30.661757,
        1.,
    ]);
    let mut root = PlacedWorldModelCollision::prepare_transform(model, transform)?;
    let map = horizon_map()?;
    for (x, group) in [(1500., 132), (1550., 132), (1600., 133)] {
        let eye = Vec3::new(x, -4400., 35.);
        // Isolate the resident WMO registration boundary; ADT occlusion is not
        // supplied here. These are reproducible probes, not the screenshot pose.
        let mut query = WorldModelCameraRegistrationQuery::new(eye, eye - Vec3::Z * 1760., 1.)?;
        query.probe_root(owner(1), WorldModelRegistrationKind::Static, &mut root)?;
        let registration = query.finish().ok_or("city camera registration")?;
        assert_eq!(registration.group, group);
        let camera = WorldCamera::stock(eye, eye + Vec3::X, Vec3::Z, 777.).frame(16. / 9.)?;
        let source = camera.camera();
        let frame = WorldSceneCameraFrame::perspective(
            eye,
            eye + Vec3::X,
            Vec3::X,
            Vec3::Z,
            source
                .vertical_field_of_view_radians()
                .ok_or("perspective camera")?,
            camera.aspect_ratio(),
            [source.near_clip(), source.far_clip()],
        )?;
        let mut scene = WorldSceneAdmission::default();
        scene
            .sky
            .seed(root.model(), [Some(group), registration.secondary_group]);
        assert!(
            scene
                .record_camera_root(&root, frame, registration, owner(1))?
                .is_none()
        );
        assert!(scene.sky.has_window());
        assert!(
            scene
                .horizon_frame(&map, camera, Vec3::ONE, WorldHorizonScale::default())?
                .is_none()
        );
    }
    Ok(())
}

/// One authored WDL tile makes publication independent of installed terrain data.
fn horizon_map() -> Result<Arc<TerrainLowDetailMap>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let mut offsets = vec![0; 4096 * 4];
    offsets[..4].copy_from_slice(&16404_u32.to_le_bytes());
    for (magic, data) in [
        (b"REVM", 18_u32.to_le_bytes().to_vec()),
        (b"FOAM", offsets),
        (b"ERAM", vec![0; 1090]),
    ] {
        super::chunk(&mut bytes, magic, &data);
    }
    let decoded = TerrainLowDetail::decode(&AssetPath::new("World/Maps/Flat/Flat.wdl")?, &bytes)?;
    Ok(Arc::new(TerrainLowDetailMap::new(&decoded)))
}
