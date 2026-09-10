//! Equipment instances follow native component and enchantment lifetimes.

use super::*;
use crate::application::player_coordinator::RuntimePlayerPresentation;
use solarity_rendering::{
    CharacterAttachmentPoint, CharacterComponentTextureLevel, WorldCameraFrame,
};

#[test]
fn equipped_instances_survive_material_updates_and_follow_component_replacement()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_equipment()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    for guid in [7, 20] {
        add_unit(&mut world, guid, ObjectKind::Player, u8::from(guid == 20))?;
        fields(
            &mut world,
            guid,
            &[(122, 1), (283, 1000), (287, 2000), (313, 3000), (314, 900)],
        )?;
    }
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    let camera = WorldCamera::orthographic(
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    for placement in &frame.placements {
        if matches!(
            placement.owner,
            M2GpuPlacementOwner::PlayerItem { .. } | M2GpuPlacementOwner::PlayerItemVisual { .. }
        ) {
            let playback = placement
                .playback
                .as_ref()
                .ok_or("component playback")?
                .borrow();
            assert_eq!(
                playback.sequence, 1,
                "zero-weight variation zero is not selected"
            );
            assert_eq!(
                playback
                    .script_timer
                    .ok_or("component default timer")?
                    .start_time_ms(),
                placement.last_effect_time_ms.wrapping_add(1),
            );
            assert!(playback.script_blend.is_none());
        }
    }
    for time in [100.0, 300.0] {
        advance(&mut frame, &renderer, camera, time, &mut random)?;
    }
    let opacity = frame
        .placements
        .iter()
        .find(|placement| placement.owner == M2GpuPlacementOwner::RemotePlayerBody { guid: 20 })
        .and_then(|placement| placement.entity_opacity.as_ref())
        .ok_or("remote opacity")?;
    let opacity = Rc::clone(opacity);
    let fading_alpha = opacity.opacity();
    assert!(fading_alpha > 0.29 && fading_alpha < 0.3);
    assert_character_opacity(&frame, 20, &opacity)?;
    assert!(
        frame
            .visible_draws
            .iter()
            .any(|draw| (draw.material().alpha() - fading_alpha).abs() < 1e-6),
        "the primary fade must reach GPU mesh material packets"
    );
    // 4EAA70 passes model, texture, visual, and particle-color inputs to the
    // child. ItemDisplayInfo flags 0x40/0x80/0x100 do not replace its animation
    // or reflect its authored transform in this build.
    for placement in &frame.placements {
        let expected_animation = match placement.owner {
            M2GpuPlacementOwner::RemotePlayerBody { guid: 20 } => 96,
            M2GpuPlacementOwner::PlayerItem { .. }
            | M2GpuPlacementOwner::PlayerItemVisual { .. } => {
                assert_eq!(
                    placement.orientation,
                    solarity_rendering::M2ModelOrientation::Authored
                );
                0
            }
            _ => continue,
        };
        assert_eq!(
            placement
                .playback
                .as_ref()
                .ok_or("playback")?
                .borrow()
                .animation_id,
            expected_animation,
            "{:?}",
            placement.owner
        );
    }
    let before = snapshots(&frame)?;
    assert_eq!(
        before.len(),
        16,
        "four components and four effects on each player"
    );
    for snapshot in &before {
        assert!(!snapshot.effects.particles.is_empty());
        assert!(snapshot.effects.ribbons.len() > 1);
        assert!(snapshot.event_time > 0);
    }
    let mask = renderer.upload_stock_m2_white()?;
    let player = presentation
        .resident_frame_input()
        .ok_or("portrait player")?;
    assert!(frame.render_player_portrait(&mut renderer, &player, mask)?);
    let portrait = renderer
        .unit_portrait_texture("player")
        .ok_or("portrait image")?;
    assert_visible_portrait(&mut renderer, portrait)?;
    assert_eq!(
        snapshots(&frame)?,
        before,
        "portrait sampling preserves live equipment timers and effects"
    );
    presentation.set_component_texture_level(
        CharacterComponentTextureLevel::new(8).ok_or("texture level")?,
    );
    let expected = random;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(
        random, expected,
        "material changes consume no component initialization rolls"
    );
    assert_eq!(snapshots(&frame)?, before);
    assert_character_opacity(&frame, 20, &opacity)?;
    assert_eq!(
        opacity.opacity(),
        fading_alpha,
        "material replacement preserves the fade clock"
    );
    let player = presentation
        .resident_frame_input()
        .ok_or("updated portrait player")?;
    assert!(frame.render_player_portrait(&mut renderer, &player, mask)?);
    assert_eq!(renderer.unit_portrait_texture("player"), Some(portrait));
    assert_visible_portrait(&mut renderer, portrait)?;
    assert_eq!(
        snapshots(&frame)?,
        before,
        "appearance recapture leaves retained equipment untouched"
    );
    advance(&mut frame, &renderer, camera, 350.0, &mut random)?;

    // A later component with a missing authored body link must not detach
    // the earlier components already selected for retention in this plan.
    let before = snapshots(&frame)?;
    fields(&mut world, 20, &[(315, 3000)])?;
    presentation.synchronize_remote_players(Some(&world))?;
    let failure = frame.replace_remote_players(
        &mut renderer,
        &presentation.resident_remote_player_frame_inputs(),
        &mut random,
    );
    assert!(matches!(failure, Err(crate::application::terrain_frame::RuntimeTerrainFrameError::MissingPlayerM2Attachment { attachment_id: 2, .. })));
    assert_eq!(snapshots(&frame)?, before);
    fields(&mut world, 20, &[(315, 0)])?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(snapshots(&frame)?, before);

    // A different weapon item recreates the component even at the same model path.
    let before = snapshots(&frame)?;
    fields(&mut world, 20, &[(313, 3001)])?;
    let expected = rolls(random, 4); // One variation and cycle draw for item and effect.
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(random, expected);
    assert_replaced(&frame, &before, |owner| {
        belongs_to(owner, 20, CharacterAttachmentPoint::HandRight)
    })?;
    advance(&mut frame, &renderer, camera, 400.0, &mut random)?;

    // 6D6BA0/4EA8F0 rebuild the enchant children while keeping their item.
    // 902 selects the same effect model as 900: path equality cannot retain it.
    let before = snapshots(&frame)?;
    fields(&mut world, 20, &[(314, 902)])?;
    let expected = rolls(random, 2);
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(random, expected);
    assert_replaced(&frame, &before, |owner| {
        matches!(
            owner,
            M2GpuPlacementOwner::PlayerItemVisual {
                guid: 20,
                item_point: CharacterAttachmentPoint::HandRight,
                ..
            }
        )
    })?;

    // 4EF020/4EF710 reuse matching helmet/shoulder model names even when
    // the new displays declare different display flags and item visuals.
    let before = snapshots(&frame)?;
    fields(&mut world, 7, &[(283, 1001), (287, 2001)])?;
    let expected = random;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(random, expected);
    assert_eq!(snapshots(&frame)?, before);

    // Changing either shoulder model replaces both members of the pair.
    let before = snapshots(&frame)?;
    fields(&mut world, 20, &[(287, 2002)])?;
    let expected = rolls(random, 8); // Two shoulders and their two effects.
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(random, expected);
    assert_replaced(&frame, &before, |owner| {
        belongs_to(owner, 20, CharacterAttachmentPoint::ShoulderLeft)
            || belongs_to(owner, 20, CharacterAttachmentPoint::ShoulderRight)
    })?;
    advance(&mut frame, &renderer, camera, 600.0, &mut random)?;

    // A one-sided shoulder remains a live component during a later atlas update.
    fields(&mut world, 20, &[(287, 2003)])?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert!(!frame.placements.iter().any(|placement| belongs_to(
        placement.owner,
        20,
        CharacterAttachmentPoint::ShoulderLeft
    )));
    advance(&mut frame, &renderer, camera, 700.0, &mut random)?;
    let before = snapshots(&frame)?;
    presentation.set_component_texture_level(
        CharacterComponentTextureLevel::new(9).ok_or("texture level")?,
    );
    let expected = random;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(random, expected);
    assert_eq!(snapshots(&frame)?, before);

    world.remove_object(20)?;
    add_unit(&mut world, 20, ObjectKind::Player, 0)?;
    fields(
        &mut world,
        20,
        &[(122, 1), (283, 1000), (287, 2003), (313, 3001), (314, 902)],
    )?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_replaced(&frame, &before, |owner| {
        matches!(
            owner,
            M2GpuPlacementOwner::PlayerItem { guid: 20, .. }
                | M2GpuPlacementOwner::PlayerItemVisual { guid: 20, .. }
        )
    })?;

    // A component created long after world entry begins with its own creation
    // timestamp, not the frame's last sampled tick. The fixture emits 20/s.
    frame.animation_started_at = std::time::Instant::now() - std::time::Duration::from_secs(20);
    fields(&mut world, 20, &[(313, 3000)])?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    let owner = M2GpuPlacementOwner::PlayerItem {
        guid: 20,
        point: CharacterAttachmentPoint::HandRight,
    };
    let created = effects(&frame, owner)?.last_update_ms;
    assert!(created >= 20_000);
    let placement = frame
        .placements
        .iter()
        .find(|placement| placement.owner == owner)
        .ok_or("new item")?;
    let playback = placement
        .playback
        .as_ref()
        .ok_or("new item playback")?
        .borrow();
    assert_eq!(
        playback
            .script_timer
            .ok_or("new item timer")?
            .start_time_ms(),
        created + 1
    );
    drop(playback);
    advance(
        &mut frame,
        &renderer,
        camera,
        (created + 100) as f32,
        &mut random,
    )?;
    let before = effects(&frame, owner)?;
    assert_eq!(
        before.particles.len(),
        2,
        "only 100ms of emission since creation"
    );

    // Skipping the model's effect update leaves its timestamp and history
    // untouched. Returning to view includes the complete skipped interval.
    let hidden_camera = WorldCamera::orthographic(
        Vec3::new(8.0, 1000.0, 0.0),
        Vec3::Y * 1000.0,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    advance(
        &mut frame,
        &renderer,
        hidden_camera,
        (created + 1000) as f32,
        &mut random,
    )?;
    assert_eq!(effects(&frame, owner)?, before);
    advance(
        &mut frame,
        &renderer,
        camera,
        (created + 1600) as f32,
        &mut random,
    )?;
    let after = effects(&frame, owner)?;
    assert!(
        (after.particles[0].age_seconds() - before.particles[0].age_seconds() - 1.5).abs()
            < 0.00001
    );
    assert!(
        (after.ribbons[0].age_seconds() - before.ribbons[0].age_seconds() - 1.5).abs() < 0.00001
    );
    Ok(())
}

pub(super) fn fields(
    world: &mut ActiveWorld,
    guid: u64,
    values: &[(u16, u32)],
) -> Result<(), Box<dyn Error>> {
    world.update_fields(guid, values.iter().copied())?;
    solarity_systems::project_object_fields(world, guid, values.iter().copied())?;
    Ok(())
}

fn publish(
    presentation: &mut RuntimePlayerPresentation,
    world: &ActiveWorld,
    frame: &mut M2Frame,
    renderer: &mut VulkanRenderer,
    random: &mut CrtRand,
) -> Result<(), Box<dyn Error>> {
    presentation.synchronize(Some(world))?;
    presentation.synchronize_remote_players(Some(world))?;
    frame.replace_player(renderer, presentation.resident_frame_input(), random)?;
    frame.replace_remote_players(
        renderer,
        &presentation.resident_remote_player_frame_inputs(),
        random,
    )?;
    Ok(())
}

fn assert_character_opacity(
    frame: &M2Frame,
    guid: u64,
    owner: &Rc<crate::application::entity_opacity::EntityOpacityOwner>,
) -> Result<(), Box<dyn Error>> {
    let mut children = 0;
    for placement in &frame.placements {
        if super::super::super::placement_owner_guid(placement.owner) != Some(guid) {
            continue;
        }
        let opacity = placement
            .entity_opacity
            .as_ref()
            .ok_or("attached opacity")?;
        assert!(Rc::ptr_eq(opacity, owner), "{:?}", placement.owner);
        if matches!(
            placement.owner,
            M2GpuPlacementOwner::PlayerItem { .. } | M2GpuPlacementOwner::PlayerItemVisual { .. }
        ) {
            children += 1;
        }
    }
    assert_eq!(
        children, 8,
        "all equipment and enchant children share the primary scalar"
    );
    Ok(())
}

pub(super) fn advance(
    frame: &mut M2Frame,
    renderer: &VulkanRenderer,
    camera: WorldCameraFrame,
    time: f32,
    random: &mut CrtRand,
) -> Result<(), Box<dyn Error>> {
    frame.prepare_visible_draws(
        renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        solarity_rendering::M2TransparentPass::One,
        Vec3::ZERO,
        time,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        random,
        None,
    )?;
    Ok(())
}

fn rolls(mut random: CrtRand, count: usize) -> CrtRand {
    for _ in 0..count {
        let _ = random.next_u15();
    }
    random
}

#[derive(Debug, PartialEq)]
struct ComponentSnapshot {
    owner: M2GpuPlacementOwner,
    source: usize,
    effects: UnitEffectsSnapshot,
    event_time: u32,
    timer: solarity_rendering::M2ModelSequenceTimer,
    orientation: solarity_rendering::M2ModelOrientation,
}

fn snapshots(frame: &M2Frame) -> Result<Vec<ComponentSnapshot>, Box<dyn Error>> {
    let mut snapshots = frame
        .placements
        .iter()
        .filter(|placement| {
            matches!(
                placement.owner,
                M2GpuPlacementOwner::PlayerItem { .. }
                    | M2GpuPlacementOwner::PlayerItemVisual { .. }
            )
        })
        .map(|placement| {
            let playback = placement
                .playback
                .as_ref()
                .ok_or("component playback")?
                .borrow();
            Ok(ComponentSnapshot {
                owner: placement.owner,
                source: unit_source(frame, placement.owner)?,
                effects: effects(frame, placement.owner)?,
                event_time: playback.previous_event_scene_time_ms,
                timer: playback.script_timer.ok_or("component timer")?,
                orientation: placement.orientation,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    snapshots.sort_by_key(|snapshot| match snapshot.owner {
        M2GpuPlacementOwner::PlayerItem { guid, point } => Some((guid, point.id(), 0)),
        M2GpuPlacementOwner::PlayerItemVisual {
            guid,
            item_point,
            effect_point,
        } => Some((guid, item_point.id(), effect_point + 1)),
        _ => None,
    });
    Ok(snapshots)
}

fn belongs_to(owner: M2GpuPlacementOwner, guid: u64, point: CharacterAttachmentPoint) -> bool {
    match owner {
        M2GpuPlacementOwner::PlayerItem {
            guid: candidate,
            point: candidate_point,
        } => candidate == guid && candidate_point == point,
        M2GpuPlacementOwner::PlayerItemVisual {
            guid: candidate,
            item_point,
            ..
        } => candidate == guid && item_point == point,
        _ => false,
    }
}

fn assert_replaced(
    frame: &M2Frame,
    before: &[ComponentSnapshot],
    replaced: impl Fn(M2GpuPlacementOwner) -> bool,
) -> Result<(), Box<dyn Error>> {
    let after = snapshots(frame)?;
    assert_eq!(after.len(), before.len());
    for current in &after {
        let previous = before
            .iter()
            .find(|previous| previous.owner == current.owner)
            .ok_or("previous component")?;
        if replaced(current.owner) {
            assert_ne!(current.source, previous.source);
            let fading_previous = frame.placements.iter().any(|placement| {
                placement.source_index == previous.source
                    && placement
                        .retirement
                        .as_ref()
                        .is_some_and(|retired| retired.original_owner == previous.owner)
            });
            // GUID reuse creates a fresh component while the removed hierarchy
            // retains its old GPU resources for disappearance interpolation.
            assert_eq!(frame.sources[previous.source].is_some(), fading_previous);
            assert!(current.effects.particles.is_empty());
            assert!(current.effects.ribbons.is_empty());
            assert_eq!(current.event_time, current.effects.last_update_ms);
            assert_eq!(
                current.timer.start_time_ms(),
                current.event_time.wrapping_add(1)
            );
        } else {
            assert_eq!(current, previous);
        }
    }
    Ok(())
}

fn assert_visible_portrait(
    renderer: &mut VulkanRenderer,
    portrait: solarity_rendering::UiPortraitTextureHandle,
) -> Result<(), Box<dyn Error>> {
    use solarity_rendering::{
        UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiSampledTexture, UiSamplerInfo,
        UiShaderSource, UiTextureAddressMode, UiTextureResidency,
    };
    let plan = UiMeshPlan::prepare(
        [128.0, 128.0],
        [UiRenderQuad::new(
            0,
            UiRenderSource::UnitPortrait("player".to_owned()),
            UiRenderBlend::Alpha,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            UiTextureResidency::Blocking,
            false,
            [0.0, 0.0, 128.0, 128.0],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
            [[1.0; 4]; 4],
        )]
        .into_iter(),
    )?;
    let mesh = renderer.upload_ui_mesh(&plan)?;
    let pipeline = renderer.prepare_ui_pipeline(UiShaderSource::Texture, UiRenderBlend::Alpha)?;
    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
    ))?;
    let set =
        renderer.prepare_ui_texture_sets(&[UiSampledTexture::portrait(portrait, sampler)])?[0];
    let draw = renderer.prepare_ui_draw(mesh, pipeline, Some(set), &plan, 0)?;
    renderer.request_frame_capture()?;
    renderer.present_ui([128.0, 128.0], &[draw])?;
    let captured = renderer
        .take_captured_frame()?
        .ok_or("missing portrait pixels")?;
    let visible = captured
        .rgba8()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3].iter().any(|channel| *channel > 32))
        .count();
    assert!(
        visible > 100,
        "frozen equipped geometry must remain visible without world fog: {visible} pixels"
    );
    Ok(())
}
