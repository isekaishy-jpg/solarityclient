//! Opt-in real-archive verification for the NPC model that failed world entry.

use super::*;
use solarity_asset::CreatureCatalog;

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with the user's 3.3.5a archives"]
fn stock_goblin_displays_prepare_visible_gpu_draws() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let catalog = CreatureCatalog::load(&mut store)?;
    let displays: Vec<_> = catalog
        .displays()
        .iter()
        .filter(|display| {
            catalog
                .model(display.model_id())
                .and_then(|model| model.model_path())
                .is_some_and(|path| {
                    matches!(
                        path.as_str(),
                        "CHARACTER\\GOBLIN\\MALE\\GOBLINMALE.M2"
                            | "CHARACTER\\GOBLIN\\MALE\\GOBLINMALE.MDX"
                    )
                })
        })
        .map(|display| display.id())
        .collect();
    assert!(!displays.is_empty());
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut presentation = unit_presentation_from_store(store)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        animations,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    let camera = WorldCamera::orthographic(
        Vec3::new(8., 0., 0.),
        Vec3::ZERO,
        Vec3::Z,
        [-4., 4.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    for display in &displays {
        solarity_systems::project_object_fields(&mut world, 30, [(67, *display), (68, *display)])?;
        presentation
            .synchronize_creatures(Some(&world))
            .map_err(|error| format!("display {display} residency: {error}"))?;
        frame
            .replace_creatures(
                &mut renderer,
                &presentation.resident_creature_frame_inputs(),
                &mut random,
            )
            .map_err(|error| format!("display {display} GPU: {error}"))?;
        let draws = frame
            .prepare_visible_draws(
                &renderer,
                WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
                camera,
                solarity_rendering::M2TransparentPass::One,
                Vec3::ZERO,
                100.,
                M2CameraEffectScale::EXTERNAL_CAMERA,
                &mut random,
                None,
            )
            .map_err(|error| format!("display {display} draws: {error}"))?;
        assert!(
            !draws.draws.is_empty(),
            "display {display} has no visible draws"
        );
    }
    eprintln!(
        "Verified {} stock Goblin male displays through visible GPU draw preparation",
        displays.len()
    );
    Ok(())
}
