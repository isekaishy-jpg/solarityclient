//! Decoded root selection and independent primary sky bank lifetime.

use super::*;

#[test]
fn sky_registration_seed_and_camera_root_replacement_follow_native_order()
-> Result<(), Box<dyn Error>> {
    let mut scene = WorldSceneAdmission::default();
    for flags in [0, 8, 0x40, 0x100, 0x40000, 0x10000] {
        let root = scene_root_with_skybox(flags, 0, false, Some("A.m2"))?;
        for groups in [[Some(0), None], [None, Some(0)]] {
            scene.sky.begin();
            scene.sky.seed(root.model(), groups);
            assert_eq!(scene.sky.window()?.is_some(), flags & 0x40140 != 0);
        }
    }
    let missing = floor_root(0x40000, 0x40000)?;
    scene.sky.begin();
    scene.sky.seed(missing.model(), [Some(0), None]);
    assert!(!scene.sky.has_window());
    let primary = owner(1);
    let secondary = owner(2);
    let a = scene_root_with_skybox(0x40000, 0x40000, false, Some("A.m2"))?;
    let b = scene_root_with_skybox(0x40000, 0x40000, false, Some("B.m2"))?;
    scene.record_camera_root(&a, camera()?, registration(secondary), primary)?;
    assert_eq!(scene.sky.skybox(), Some("A.m2"));
    assert!(scene.sky.window()?.is_none()); // Dual-root traversal discarded the seed.
    scene.record_camera_root(&b, camera()?, registration(primary), primary)?;
    assert_eq!(scene.sky.skybox(), Some("B.m2"));
    scene.record_camera_root(&missing, camera()?, registration(primary), primary)?;
    assert_eq!(scene.sky.skybox(), None);
    scene.record_camera_root(&a, camera()?, registration(secondary), primary)?;
    let ordinary = floor_root(0, 0)?;
    scene.record_camera_root(&ordinary, camera()?, registration(primary), primary)?;
    assert_eq!(scene.sky.skybox(), Some("A.m2")); // No qualifying primary callback.
    scene.sky.begin();
    scene.sky.outdoors();
    assert_eq!(scene.sky.skybox(), None);
    assert_eq!(
        scene.sky.window()?,
        Some(solarity_rendering::WorldSkyWindow::FULL)
    );
    Ok(())
}

#[test]
fn primary_sky_portals_survive_secondary_reset_and_merge_with_registration_seed()
-> Result<(), Box<dyn Error>> {
    let root = scene_root(0, 0, true)?;
    let eye = Vec3::new(10., 0., 1.);
    let camera = WorldSceneCameraFrame::perspective(
        eye,
        eye - Vec3::X,
        -Vec3::X,
        Vec3::Z,
        1.,
        1.5,
        [0.1, 1000.],
    )?;
    let primary = owner(1);
    let mut registration = registration(owner(2));
    registration.group = 1;
    let mut scene = WorldSceneAdmission::default();
    assert!(
        scene
            .record_camera_root(&root, camera, registration, primary)?
            .is_some()
    );
    assert!(scene.sky.window()?.is_none());
    registration.owner = primary;
    scene.record_camera_root(&root, camera, registration, primary)?;
    let portal = scene
        .sky
        .window()?
        .ok_or("primary portal did not admit sky")?;
    assert_ne!(portal, solarity_rendering::WorldSkyWindow::FULL);
    scene.sky.outdoors();
    scene.record_camera_root(&root, camera, registration, primary)?;
    assert_eq!(
        scene.sky.window()?,
        Some(solarity_rendering::WorldSkyWindow::FULL)
    );
    Ok(())
}
