//! Authored starburst pixels through the complete world/glow compositor.

use glam::{Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale,
};
use solarity_rendering::{
    BlpColorSpace, VulkanBootstrap, WorldCamera, WorldCelestialDraw, WorldCelestialFrame,
    WorldCelestialMesh, WorldCelestials, WorldFrameScreenEffect, WorldGlareEnvironment,
    WorldGlareFrame,
};

/// Real sunGlare transparency and rays must contribute beyond the ordinary disc.
#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
#[allow(unsafe_code)] // SDL transfers the hidden surface to the Vulkan renderer.
fn installed_sunburst_washes_out_disc_and_fades_with_clouds()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL lock poisoned")?;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity installed sunburst", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This live window supplied the extensions and outlives the surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let mut textures = Vec::new();
    for path in ["Textures/sunCenter.blp", "Textures/sunGlare.blp"] {
        let source = BlpTextureSource::load(&mut store, &AssetPath::new(path)?)?;
        textures.push(renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?);
    }
    let bodies = WorldCelestials::sample(0.5, 20000., Vec3::ZERO).bodies();
    let camera = WorldCamera::stock(Vec3::ZERO, bodies[0].position(), Vec3::Z, 1000.).frame(1.)?;
    let disc = WorldCelestialMesh::new(bodies[0], Vec3::ZERO, 0xffffffff);
    let hidden = WorldCelestialMesh::new(bodies[1], Vec3::ZERO, 0);
    let celestials = WorldCelestialFrame::new([
        WorldCelestialDraw::new(&disc, bodies[0], textures[0], camera),
        WorldCelestialDraw::new(&hidden, bodies[1], textures[0], camera),
        WorldCelestialDraw::new(&hidden, bodies[2], textures[0], camera),
    ]);
    let mut brightness = Vec::new();
    for (case, cloud) in [1., 0., 0.9, 1.].into_iter().enumerate() {
        for step in 0..16 {
            let scene = super::world_scene(camera)
                // Controlled background isolates the authored flare and stock
                // glow composition; this is not a map/weather screenshot oracle.
                .with_background_color(Vec4::new(0.6, 0.7, 0.8, 1.))
                .with_celestials(celestials)
                .with_glare(WorldGlareFrame {
                    camera,
                    bodies: [bodies[0], bodies[1]],
                    textures: [textures[1]; 2],
                    colors: [0xffffffff, 0],
                    environment: WorldGlareEnvironment {
                        day: 0.5,
                        elapsed_seconds: 0.1,
                        cloud_alpha: [cloud; 2],
                        liquid_depth: None,
                        skybox_weight: 0.,
                    },
                })
                .with_screen_effect(Some(WorldFrameScreenEffect::normal(0.4, None, false, 0)));
            if step == 15 {
                renderer.request_frame_capture()?;
            }
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
            if step == 15 {
                let capture = renderer
                    .take_captured_frame()?
                    .ok_or("missing sunburst capture")?;
                // Twenty pixels from center lies outside the one-unit sun disc.
                let pixel = &capture.rgba8()[(128 * 256 + 148) * 4..][..3];
                brightness.push(pixel.iter().map(|&v| u32::from(v)).sum::<u32>());
                if case == 1 {
                    assert!(pixel.iter().all(|&v| v >= 250), "clear sunburst: {pixel:?}");
                }
                if let Some(directory) = std::env::var_os("SOLARITY_SKYBOX_CAPTURE_DIR") {
                    std::fs::write(
                        std::path::Path::new(&directory).join(format!("sunburst-{case}.rgba")),
                        capture.rgba8(),
                    )?;
                }
            }
        }
    }
    assert!(
        brightness[0] < brightness[2] && brightness[2] < brightness[1],
        "cloud response: {brightness:?}"
    );
    assert_eq!(
        brightness[0], brightness[3],
        "cloud cover did not restore the baseline"
    );
    Ok(())
}
