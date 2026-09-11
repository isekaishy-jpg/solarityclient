//! Opt-in real-archive verification for NPC bodies and separate armor models.

use super::*;
use solarity_asset::CreatureCatalog;

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with the user's 3.3.5a archives"]
fn stock_npc_displays_prepare_visible_gpu_draws() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let catalog = CreatureCatalog::load(&mut store)?;
    let item_displays = solarity_asset::ItemDisplayCatalog::load(&mut store)?;
    let mut displays: Vec<_> = catalog
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
    let mut orcs = catalog
        .displays()
        .iter()
        .filter_map(|display| {
            let appearance = catalog.resolve_model(display.id()).ok()?;
            let extra = appearance.extra()?;
            let gear = extra.npc_item_display_ids();
            (extra.race_id() == 2 && extra.gender_id() == 0 && gear[0] != 0 && gear[1] != 0)
                .then_some(display.id())
        })
        .take(4)
        .collect::<Vec<_>>();
    assert_eq!(orcs.len(), 4, "stock armored male Orc appearances");
    // Orc armor includes deliberately one-sided shoulders. Also select two
    // complete pairs so both authored channels reach the stock asset check.
    let pairs = catalog
        .displays()
        .iter()
        .filter_map(|display| {
            let appearance = catalog.resolve_model(display.id()).ok()?;
            let extra = appearance.extra()?;
            let gear = extra.npc_item_display_ids();
            let helmet = item_displays.display(gear[0])?;
            let shoulder = item_displays.display(gear[1])?;
            (extra.race_id() == 2
                && extra.gender_id() == 0
                && !helmet.model_names()[0].is_empty()
                && shoulder.model_names().iter().all(|name| !name.is_empty()))
            .then_some(display.id())
        })
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(
        pairs.len(),
        2,
        "stock Orcs with a helmet and two shoulder models"
    );
    orcs.extend(pairs);
    orcs.sort_unstable();
    orcs.dedup();
    displays.extend_from_slice(&orcs);
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
            .synchronize_creatures(Some(&world), |_| None)
            .map_err(|error| format!("display {display} residency: {error}"))?;
        if orcs.contains(display) {
            let appearance = catalog.resolve_model(*display)?;
            let gear = appearance
                .extra()
                .ok_or("stock NPC Extra row")?
                .npc_item_display_ids();
            let helmet = item_displays
                .display(gear[0])
                .ok_or("stock helmet display")?;
            let shoulder = item_displays
                .display(gear[1])
                .ok_or("stock shoulder display")?;
            let inputs = presentation.resident_creature_frame_inputs();
            let input = inputs.first().ok_or("stock NPC frame input")?;
            for (point, model) in [
                (
                    solarity_rendering::CharacterAttachmentPoint::Helmet,
                    helmet.model_names()[0],
                ),
                (
                    solarity_rendering::CharacterAttachmentPoint::ShoulderLeft,
                    shoulder.model_names()[1],
                ),
                (
                    solarity_rendering::CharacterAttachmentPoint::ShoulderRight,
                    shoulder.model_names()[0],
                ),
            ] {
                assert_eq!(
                    input.attachments().iter().any(|item| item.point() == point),
                    !model.is_empty(),
                    "stock Orc display {display} {point:?}: authored model {model:?}"
                );
            }
        }
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
        "Verified {} stock NPC displays, including armored Orcs {orcs:?}, through visible GPU draw preparation",
        displays.len(),
    );
    Ok(())
}
